//! macOS: let the page render at the display's refresh rate (ProMotion, up to 120 Hz).
//!
//! WKWebView limits a page's rendering updates (requestAnimationFrame) to about 60 per second
//! even on 120 Hz displays, through the WebKit feature "PreferPageRenderingUpdatesNear60FPSEnabled"
//! (on by default). Monaco renders its own scrolling on those frames, so the editor scrolled at
//! 60 Hz on the owner's 120 Hz display (measured: frames every 16.7 ms, visible and hidden).
//!
//! The feature can only be turned off through WebKit's private SPI (`+[WKPreferences _features]`,
//! `-[WKPreferences _setEnabled:forFeature:]`); the owner approved a private API for this. Every
//! selector is checked with `respondsToSelector:` first: if a WebKit release drops or renames
//! them, nothing is called and the page keeps rendering at 60 Hz.

use objc2::runtime::{AnyClass, AnyObject, Bool, Sel};
use objc2::{msg_send, sel};
use std::ffi::{c_char, CStr};

const FEATURE: &CStr = c"PreferPageRenderingUpdatesNear60FPSEnabled";

/// Turns the 60 fps preference off for `webview` (a `WKWebView *`). Returns what happened.
/// Must run on the main thread (Tauri's `with_webview` callback does).
#[allow(unsafe_code)]
pub fn allow_full_refresh_rate(webview: *mut std::ffi::c_void) -> Result<(), &'static str> {
    let webview = webview.cast::<AnyObject>();
    if webview.is_null() {
        return Err("no WKWebView");
    }
    let Some(prefs_class) = AnyClass::get(c"WKPreferences") else {
        return Err("WKPreferences missing");
    };
    // SAFETY: every message below goes to an Objective-C object of the expected class (a live
    // WKWebView handed over by Tauri on the main thread, its configuration and preferences, an
    // NSArray and its elements, NSString keys) and every private selector is checked with
    // respondsToSelector: before it is sent. Return types match the documented signatures.
    unsafe {
        let responds = |obj: *const AnyObject, s: Sel| -> bool {
            let r: Bool = msg_send![obj, respondsToSelector: s];
            r.as_bool()
        };
        let class_obj = (prefs_class as *const AnyClass).cast::<AnyObject>();
        if !responds(class_obj, sel!(_features)) {
            return Err("+[WKPreferences _features] missing");
        }
        let config: *mut AnyObject = msg_send![webview, configuration];
        let prefs: *mut AnyObject = msg_send![config, preferences];
        if prefs.is_null() || !responds(prefs, sel!(_setEnabled:forFeature:)) {
            return Err("-[WKPreferences _setEnabled:forFeature:] missing");
        }
        let features: *mut AnyObject = msg_send![prefs_class, _features];
        if features.is_null() {
            return Err("no WebKit features");
        }
        let count: usize = msg_send![features, count];
        for i in 0..count {
            let feature: *mut AnyObject = msg_send![features, objectAtIndex: i];
            if feature.is_null() || !responds(feature, sel!(key)) {
                continue;
            }
            let key: *mut AnyObject = msg_send![feature, key];
            if key.is_null() {
                continue;
            }
            let utf8: *const c_char = msg_send![key, UTF8String];
            if !utf8.is_null() && CStr::from_ptr(utf8) == FEATURE {
                let _: () = msg_send![prefs, _setEnabled: Bool::NO, forFeature: feature];
                return Ok(());
            }
        }
    }
    Err("feature PreferPageRenderingUpdatesNear60FPSEnabled not found")
}
