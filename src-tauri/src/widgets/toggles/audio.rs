// COM interface methods keep their Windows names.
#![allow(non_snake_case)]

//! Output volume, mute and the default output device, through Core Audio.
//!
//! Everything runs on the shell worker thread. Core Audio calls back on its
//! own threads when the volume or the device list changes; those callbacks
//! only ask the worker for a refresh.

use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use windows::Win32::Media::Audio::{
    AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE, DEVICE_STATE_ACTIVE, EDataFlow, ERole, IMMDevice,
    IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl, MMDeviceEnumerator,
    eCommunications, eConsole, eMultimedia, eRender,
};
use windows::Win32::System::Com::StructuredStorage::{
    PROPVARIANT, PropVariantClear, PropVariantToStringAlloc,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree, STGM_READ,
};
use windows::core::{
    GUID, HRESULT, HSTRING, IUnknown, IUnknown_Vtbl, PCWSTR, PWSTR, implement, interface,
};

use crate::shell::worker;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioState {
    /// False when there is no output device.
    pub available: bool,
    /// 0 to 100.
    pub volume: u8,
    pub muted: bool,
    pub devices: Vec<OutputDevice>,
}

/// Windows has no public API for choosing the default audio device. Every
/// device switcher, including this one, uses this undocumented interface of
/// the audio policy service. It is used for exactly one call,
/// `SetDefaultEndpoint`. The methods before it are declared only to keep the
/// vtable layout right and are never called.
#[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
unsafe trait IPolicyConfig: IUnknown {
    fn GetMixFormat(&self, device: PCWSTR, format: *mut *mut c_void) -> HRESULT;
    fn GetDeviceFormat(&self, device: PCWSTR, default: i32, format: *mut *mut c_void) -> HRESULT;
    fn ResetDeviceFormat(&self, device: PCWSTR) -> HRESULT;
    fn SetDeviceFormat(&self, device: PCWSTR, endpoint: *mut c_void, mix: *mut c_void) -> HRESULT;
    fn GetProcessingPeriod(
        &self,
        device: PCWSTR,
        default: i32,
        default_period: *mut i64,
        minimum_period: *mut i64,
    ) -> HRESULT;
    fn SetProcessingPeriod(&self, device: PCWSTR, period: *mut i64) -> HRESULT;
    fn GetShareMode(&self, device: PCWSTR, mode: *mut c_void) -> HRESULT;
    fn SetShareMode(&self, device: PCWSTR, mode: *mut c_void) -> HRESULT;
    fn GetPropertyValue(
        &self,
        device: PCWSTR,
        store: i32,
        key: *const PROPERTYKEY,
        value: *mut PROPVARIANT,
    ) -> HRESULT;
    fn SetPropertyValue(
        &self,
        device: PCWSTR,
        store: i32,
        key: *const PROPERTYKEY,
        value: *mut PROPVARIANT,
    ) -> HRESULT;
    fn SetDefaultEndpoint(&self, device: PCWSTR, role: ERole) -> HRESULT;
    fn SetEndpointVisibility(&self, device: PCWSTR, visible: i32) -> HRESULT;
}

const POLICY_CONFIG_CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeCallback;

impl IAudioEndpointVolumeCallback_Impl for VolumeCallback_Impl {
    fn OnNotify(&self, _data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        request_refresh();
        Ok(())
    }
}

#[implement(IMMNotificationClient)]
struct DeviceCallback;

impl IMMNotificationClient_Impl for DeviceCallback_Impl {
    fn OnDeviceStateChanged(
        &self,
        _id: &PCWSTR,
        _state: DEVICE_STATE,
    ) -> windows::core::Result<()> {
        request_refresh();
        Ok(())
    }
    fn OnDeviceAdded(&self, _id: &PCWSTR) -> windows::core::Result<()> {
        request_refresh();
        Ok(())
    }
    fn OnDeviceRemoved(&self, _id: &PCWSTR) -> windows::core::Result<()> {
        request_refresh();
        Ok(())
    }
    fn OnDefaultDeviceChanged(
        &self,
        _flow: EDataFlow,
        _role: ERole,
        _id: &PCWSTR,
    ) -> windows::core::Result<()> {
        request_refresh();
        Ok(())
    }
    fn OnPropertyValueChanged(
        &self,
        _id: &PCWSTR,
        _key: &PROPERTYKEY,
    ) -> windows::core::Result<()> {
        Ok(())
    }
}

/// The default output device and its volume control.
struct Endpoint {
    id: String,
    volume: IAudioEndpointVolume,
    callback: IAudioEndpointVolumeCallback,
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        unsafe {
            let _ = self.volume.UnregisterControlChangeNotify(&self.callback);
        }
    }
}

struct Audio {
    enumerator: IMMDeviceEnumerator,
    /// Kept alive for as long as it is registered with the enumerator.
    _notifier: IMMNotificationClient,
    endpoint: Option<Endpoint>,
}

thread_local! {
    /// Lives on the shell worker thread only.
    static AUDIO: RefCell<Option<Audio>> = const { RefCell::new(None) };
}

static STATE: Mutex<Option<AudioState>> = Mutex::new(None);
static APP: OnceLock<AppHandle> = OnceLock::new();
static REFRESH_QUEUED: AtomicBool = AtomicBool::new(false);

unsafe fn take(value: PWSTR) -> String {
    if value.is_null() {
        return String::new();
    }
    let text = unsafe { value.to_string() }.unwrap_or_default();
    unsafe { CoTaskMemFree(Some(value.0 as *const _)) };
    text
}

fn device_id(device: &IMMDevice) -> String {
    unsafe { device.GetId().map(|id| take(id)).unwrap_or_default() }
}

fn device_name(device: &IMMDevice) -> String {
    unsafe {
        let Ok(store) = device.OpenPropertyStore(STGM_READ) else {
            return String::new();
        };
        let Ok(mut value) = store.GetValue(&PKEY_Device_FriendlyName) else {
            return String::new();
        };
        let name = PropVariantToStringAlloc(&value)
            .map(|p| take(p))
            .unwrap_or_default();
        let _ = PropVariantClear(&mut value);
        name
    }
}

fn bind_endpoint(device: &IMMDevice) -> Option<Endpoint> {
    unsafe {
        let volume: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None).ok()?;
        let callback: IAudioEndpointVolumeCallback = VolumeCallback.into();
        volume.RegisterControlChangeNotify(&callback).ok()?;
        Some(Endpoint {
            id: device_id(device),
            volume,
            callback,
        })
    }
}

fn init() -> Option<Audio> {
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER).ok()?;
        let notifier: IMMNotificationClient = DeviceCallback.into();
        let _ = enumerator.RegisterEndpointNotificationCallback(&notifier);
        Some(Audio {
            enumerator,
            _notifier: notifier,
            endpoint: None,
        })
    }
}

/// Read the current state, following the default device if it changed.
fn read(audio: &mut Audio) -> AudioState {
    unsafe {
        let default = audio
            .enumerator
            .GetDefaultAudioEndpoint(eRender, eMultimedia)
            .ok();
        let default_id = default.as_ref().map(device_id).unwrap_or_default();
        if audio.endpoint.as_ref().map(|e| e.id.as_str()) != Some(default_id.as_str()) {
            audio.endpoint = default.as_ref().and_then(bind_endpoint);
        }

        let mut devices = Vec::new();
        if let Ok(collection) = audio
            .enumerator
            .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
        {
            for index in 0..collection.GetCount().unwrap_or(0) {
                if let Ok(device) = collection.Item(index) {
                    let id = device_id(&device);
                    devices.push(OutputDevice {
                        name: device_name(&device),
                        is_default: id == default_id,
                        id,
                    });
                }
            }
        }

        match &audio.endpoint {
            Some(endpoint) => AudioState {
                available: true,
                volume: (endpoint.volume.GetMasterVolumeLevelScalar().unwrap_or(0.0) * 100.0)
                    .round()
                    .clamp(0.0, 100.0) as u8,
                muted: endpoint.volume.GetMute().is_ok_and(|m| m.as_bool()),
                devices,
            },
            None => AudioState {
                devices,
                ..Default::default()
            },
        }
    }
}

fn refresh() {
    let next = AUDIO.with_borrow_mut(|slot| {
        if slot.is_none() {
            *slot = init();
        }
        slot.as_mut().map(read).unwrap_or_default()
    });
    let changed = STATE.lock().replace(next.clone()).as_ref() != Some(&next);
    if changed && let Some(app) = APP.get() {
        let _ = app.emit("audio://changed", next);
    }
}

/// Callable from any thread, including Core Audio's callback threads.
pub fn request_refresh() {
    if !REFRESH_QUEUED.swap(true, Ordering::SeqCst) {
        worker::post(|| {
            REFRESH_QUEUED.store(false, Ordering::SeqCst);
            refresh();
        });
    }
}

pub fn start(app: &AppHandle) {
    let _ = APP.set(app.clone());
    request_refresh();
}

pub fn state() -> AudioState {
    STATE.lock().clone().unwrap_or_default()
}

fn with_endpoint(action: impl FnOnce(&IAudioEndpointVolume) + Send + 'static) {
    worker::post(move || {
        AUDIO.with_borrow(|audio| {
            if let Some(endpoint) = audio.as_ref().and_then(|a| a.endpoint.as_ref()) {
                action(&endpoint.volume);
            }
        });
    });
}

pub fn set_volume(percent: u8) {
    let level = f32::from(percent.min(100)) / 100.0;
    with_endpoint(move |volume| unsafe {
        let _ = volume.SetMasterVolumeLevelScalar(level, std::ptr::null());
    });
}

pub fn set_muted(muted: bool) {
    with_endpoint(move |volume| unsafe {
        let _ = volume.SetMute(muted, std::ptr::null());
    });
}

/// Make `id` the default output device for every role.
pub fn set_default_device(id: String) {
    worker::post(move || unsafe {
        let config: IPolicyConfig = match CoCreateInstance(&POLICY_CONFIG_CLIENT, None, CLSCTX_ALL)
        {
            Ok(config) => config,
            Err(e) => {
                log::warn!("audio: the device switching interface is unavailable: {e}");
                return;
            }
        };
        let device = HSTRING::from(id.as_str());
        for role in [eConsole, eMultimedia, eCommunications] {
            let result = config.SetDefaultEndpoint(PCWSTR(device.as_ptr()), role);
            if result.is_err() {
                log::warn!("audio: could not switch the output device: {result:?}");
                return;
            }
        }
        refresh();
    });
}
