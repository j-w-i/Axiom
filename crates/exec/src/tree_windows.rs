//! Windows process tree via a Job Object. The only module in this crate that
//! may use `unsafe`.
#![allow(unsafe_code)]

use crate::tree::ProcessTree;
use std::mem::size_of;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
};
use windows_sys::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};

/// The process is created with its main thread suspended (AUD-101): it can start nothing
/// before [`JobTree::attach`] has put it in the job; [`resume`] then lets it run.
pub const CREATE_SUSPENDED: u32 = windows_sys::Win32::System::Threading::CREATE_SUSPENDED;

/// Resumes the threads of a process created with [`CREATE_SUSPENDED`] (std does not expose the
/// main thread's handle, so its threads are found through a ToolHelp snapshot).
pub fn resume(child: &tokio::process::Child) -> std::io::Result<()> {
    let pid = child
        .id()
        .ok_or_else(|| std::io::Error::other("the process has already exited"))?;
    // SAFETY: plain call; the result is checked.
    let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snap == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error());
    }
    let mut entry = THREADENTRY32 {
        dwSize: size_of::<THREADENTRY32>() as u32,
        ..Default::default()
    };
    let mut resumed = 0;
    // SAFETY: `snap` is a valid snapshot handle and `entry` is initialised with its size.
    let mut more = unsafe { Thread32First(snap, &mut entry) } != 0;
    while more {
        if entry.th32OwnerProcessID == pid {
            // SAFETY: plain call; the handle is checked and closed below.
            let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
            if !thread.is_null() {
                // SAFETY: `thread` is a valid handle we own.
                if unsafe { ResumeThread(thread) } != u32::MAX {
                    resumed += 1;
                }
                // SAFETY: `thread` is a valid handle we own.
                unsafe { CloseHandle(thread) };
            }
        }
        // SAFETY: as for `Thread32First`.
        more = unsafe { Thread32Next(snap, &mut entry) } != 0;
    }
    // SAFETY: `snap` is a valid handle we own.
    unsafe { CloseHandle(snap) };
    if resumed == 0 {
        return Err(std::io::Error::other(
            "no thread of the process could be resumed",
        ));
    }
    Ok(())
}
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};

pub struct JobTree {
    job: HANDLE,
}

// SAFETY: a job handle is a process-wide kernel handle usable from any thread.
unsafe impl Send for JobTree {}

impl JobTree {
    pub fn attach(child: &tokio::process::Child) -> Option<Self> {
        let process = child.raw_handle()?;
        // SAFETY: null attributes and name are allowed; the result is checked.
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return None;
        }
        // SAFETY: zeroed is a valid bit pattern for this plain-data struct.
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: `job` is valid and `info` outlives the call; size matches the class.
        let ok = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const core::ffi::c_void,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        // SAFETY: both handles are valid; `process` is owned by `child`, which is alive.
        if ok == 0 || unsafe { AssignProcessToJobObject(job, process as HANDLE) } == 0 {
            // SAFETY: `job` is a valid handle we own.
            unsafe { CloseHandle(job) };
            return None;
        }
        Some(Self { job })
    }

    fn terminate_job(&mut self) {
        // SAFETY: `job` is a valid handle owned by self.
        unsafe { TerminateJobObject(self.job, 1) };
    }
}

impl ProcessTree for JobTree {
    fn terminate(&mut self) {
        self.terminate_job();
    }
    fn kill(&mut self) {
        self.terminate_job();
    }
}

impl Drop for JobTree {
    fn drop(&mut self) {
        // SAFETY: `job` is a valid handle owned by self; KILL_ON_JOB_CLOSE
        // takes down any remaining processes.
        unsafe { CloseHandle(self.job) };
    }
}
