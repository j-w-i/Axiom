//! Windows process tree via a Job Object. The only module in this crate that
//! may use `unsafe`.
#![allow(unsafe_code)]

use crate::tree::ProcessTree;
use std::mem::size_of;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
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
