#ifndef RUNNER_DROP_TARGET_H_
#define RUNNER_DROP_TARGET_H_

#include <flutter/binary_messenger.h>
#include <flutter/encodable_value.h>
#include <flutter/method_channel.h>
#include <windows.h>
#include <ole2.h>

#include <memory>
#include <string>
#include <vector>

// Tells what is dragged over a window and dropped on it
// (app/lib/shell/drop.dart).
class DropTarget : public IDropTarget {
 public:
  // Held until |Revoke|. Null where the system will not have it.
  static DropTarget* Register(HWND window, flutter::BinaryMessenger* messenger);

  // Nothing is told after this.
  void Revoke();

  // IUnknown:
  HRESULT STDMETHODCALLTYPE QueryInterface(REFIID id, void** out) override;
  ULONG STDMETHODCALLTYPE AddRef() override;
  ULONG STDMETHODCALLTYPE Release() override;

  // IDropTarget:
  HRESULT STDMETHODCALLTYPE DragEnter(IDataObject* data, DWORD keys, POINTL at,
                                      DWORD* effect) override;
  HRESULT STDMETHODCALLTYPE DragOver(DWORD keys, POINTL at,
                                     DWORD* effect) override;
  HRESULT STDMETHODCALLTYPE DragLeave() override;
  HRESULT STDMETHODCALLTYPE Drop(IDataObject* data, DWORD keys, POINTL at,
                                 DWORD* effect) override;

 private:
  DropTarget(HWND window, flutter::BinaryMessenger* messenger);
  virtual ~DropTarget() = default;

  void Tell(const char* what, std::unique_ptr<flutter::EncodableValue> args);

  LONG references_ = 1;
  HWND window_;
  std::unique_ptr<flutter::MethodChannel<flutter::EncodableValue>> channel_;

  // What is held over it are files.
  bool files_ = false;
};

#endif  // RUNNER_DROP_TARGET_H_
