#include "drop_target.h"

#include <flutter/standard_method_codec.h>
#include <shellapi.h>

#include "utils.h"

namespace {

// Files that are there, two at most: of more than one, only that there are more
// is of interest.
std::vector<std::wstring> FilesOf(IDataObject* data) {
  std::vector<std::wstring> files;
  FORMATETC format = {CF_HDROP, nullptr, DVASPECT_CONTENT, -1, TYMED_HGLOBAL};
  STGMEDIUM medium = {};
  if (data == nullptr || FAILED(data->GetData(&format, &medium))) {
    return files;
  }
  auto drop = static_cast<HDROP>(::GlobalLock(medium.hGlobal));
  if (drop != nullptr) {
    const UINT count = ::DragQueryFileW(drop, 0xFFFFFFFF, nullptr, 0);
    for (UINT i = 0; i < count && files.size() < 2; i++) {
      const UINT length = ::DragQueryFileW(drop, i, nullptr, 0);
      if (length == 0 || length > 32767) {
        continue;
      }
      std::wstring file(static_cast<size_t>(length) + 1, L'\0');
      const UINT got = ::DragQueryFileW(drop, i, file.data(), length + 1);
      file.resize(got);
      if (!file.empty()) {
        files.push_back(std::move(file));
      }
    }
    ::GlobalUnlock(medium.hGlobal);
  }
  ::ReleaseStgMedium(&medium);
  return files;
}

// Never to be moved: whoever drags it would remove it.
DWORD Taken(DWORD may) {
  if ((may & DROPEFFECT_COPY) != 0) {
    return DROPEFFECT_COPY;
  }
  return (may & DROPEFFECT_LINK) != 0 ? DROPEFFECT_LINK : DROPEFFECT_NONE;
}

}  // namespace

// static
DropTarget* DropTarget::Register(HWND window,
                                 flutter::BinaryMessenger* messenger) {
  auto target = new DropTarget(window, messenger);
  if (FAILED(::RegisterDragDrop(window, target))) {
    target->Release();
    return nullptr;
  }
  return target;
}

DropTarget::DropTarget(HWND window, flutter::BinaryMessenger* messenger)
    : window_(window),
      channel_(std::make_unique<flutter::MethodChannel<flutter::EncodableValue>>(
          messenger, "axiom/drop",
          &flutter::StandardMethodCodec::GetInstance())) {}

void DropTarget::Revoke() {
  channel_ = nullptr;
  ::RevokeDragDrop(window_);
  Release();
}

void DropTarget::Tell(const char* what,
                      std::unique_ptr<flutter::EncodableValue> args) {
  if (channel_) {
    channel_->InvokeMethod(what, std::move(args));
  }
}

HRESULT DropTarget::QueryInterface(REFIID id, void** out) {
  if (out == nullptr) {
    return E_POINTER;
  }
  if (id == IID_IUnknown || id == IID_IDropTarget) {
    *out = static_cast<IDropTarget*>(this);
    AddRef();
    return S_OK;
  }
  *out = nullptr;
  return E_NOINTERFACE;
}

ULONG DropTarget::AddRef() {
  return static_cast<ULONG>(::InterlockedIncrement(&references_));
}

ULONG DropTarget::Release() {
  const LONG left = ::InterlockedDecrement(&references_);
  if (left == 0) {
    delete this;
  }
  return static_cast<ULONG>(left);
}

HRESULT DropTarget::DragEnter(IDataObject* data, DWORD, POINTL, DWORD* effect) {
  const auto files = FilesOf(data);
  files_ = !files.empty();
  *effect = files_ ? Taken(*effect) : DROPEFFECT_NONE;
  if (files.size() == 1) {
    const DWORD is = ::GetFileAttributesW(files[0].c_str());
    if (is != INVALID_FILE_ATTRIBUTES && (is & FILE_ATTRIBUTE_DIRECTORY) != 0) {
      Tell("over", std::make_unique<flutter::EncodableValue>(
                       Utf8FromUtf16(files[0].c_str())));
    }
  }
  return S_OK;
}

HRESULT DropTarget::DragOver(DWORD, POINTL, DWORD* effect) {
  *effect = files_ ? Taken(*effect) : DROPEFFECT_NONE;
  return S_OK;
}

HRESULT DropTarget::DragLeave() {
  files_ = false;
  Tell("out", nullptr);
  return S_OK;
}

HRESULT DropTarget::Drop(IDataObject* data, DWORD, POINTL, DWORD* effect) {
  flutter::EncodableList paths;
  for (const auto& file : FilesOf(data)) {
    paths.push_back(flutter::EncodableValue(Utf8FromUtf16(file.c_str())));
  }
  files_ = false;
  *effect = paths.empty() ? DROPEFFECT_NONE : Taken(*effect);
  Tell("drop", std::make_unique<flutter::EncodableValue>(std::move(paths)));
  return S_OK;
}
