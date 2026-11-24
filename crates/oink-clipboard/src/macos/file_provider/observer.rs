//! Read-only domain observation. Unlike getUserVisibleURL, getDomains does not
//! make this process dataless-I/O exempt; URL resolution stays in the signed helper.
use super::DomainStatus;
use block2::RcBlock;
use objc2::{
    msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject, Bool, ProtocolObject},
};
use objc2_foundation::{NSArray, NSError, NSNotification, NSNotificationCenter, NSObjectProtocol, NSString};
use std::ffi::{CStr, c_char, c_int, c_void};
use std::ptr::NonNull;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::watch;

type Status = Result<Option<DomainStatus>, String>;

const FILE_PROVIDER_PATH: &CStr = c"/System/Library/Frameworks/FileProvider.framework/FileProvider";
const RTLD_LAZY: c_int = 0x1;
const RTLD_LOCAL: c_int = 0x4;

#[link(name = "System")]
unsafe extern "C" {
    fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

/// File Provider's manager forwards `getDomains…` over NSXPCConnection, whose
/// Catalina implementation rejects blocks that lack extended signature type
/// information (the block API used here cannot supply it) and throws. Domain
/// observation is a macOS 11 feature, so skip the query entirely before then.
fn system_supports_file_provider() -> bool {
    static SUPPORTED: OnceLock<bool> = OnceLock::new();
    *SUPPORTED.get_or_init(|| {
        let mut bytes = [0u8; 32];
        let mut size = bytes.len();
        // SAFETY: read-only sysctl into the supplied buffer; its size is in/out.
        let rc = unsafe {
            libc::sysctlbyname(
                c"kern.osproductversion".as_ptr(),
                bytes.as_mut_ptr().cast(),
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        if rc != 0 {
            return false;
        }
        let Ok(version) = CStr::from_bytes_until_nul(&bytes) else {
            return false;
        };
        let major = version
            .to_string_lossy()
            .split('.')
            .next()
            .and_then(|part| part.parse::<u64>().ok())
            .unwrap_or(0);
        major >= 11
    })
}

fn file_provider_handle() -> Option<*mut c_void> {
    static HANDLE: OnceLock<usize> = OnceLock::new();
    // SAFETY: dlopen only loads a system framework; the handle stays valid for
    // the process lifetime and loading it only registers its Objective-C
    // classes.
    let raw = *HANDLE.get_or_init(|| unsafe { dlopen(FILE_PROVIDER_PATH.as_ptr(), RTLD_LAZY | RTLD_LOCAL) as usize });
    (raw != 0).then_some(raw as *mut c_void)
}

/// Reads a `static NSString *` exported by FileProvider. The notification
/// constants used here appeared in macOS 11, so they are resolved at runtime
/// instead of at launch: a strong import would abort the process on 10.15.
fn notification_name(symbol: &CStr) -> Option<&'static NSString> {
    let handle = file_provider_handle()?;
    // SAFETY: `symbol` names a process-lifetime `NSString *` constant in the
    // loaded framework, and a missing symbol yields a null pointer.
    unsafe {
        let pointer = dlsym(handle, symbol.as_ptr());
        if pointer.is_null() {
            return None;
        }
        let value = *pointer.cast::<*const NSString>();
        NonNull::new(value.cast_mut()).map(|value| value.as_ref())
    }
}

#[derive(Debug)]
pub struct Observer {
    cache_token: Option<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
    token: Option<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
    receiver: watch::Receiver<Option<Status>>,
    sender: watch::Sender<Option<Status>>,
    generation: Arc<Mutex<u64>>,
}

// SAFETY: NSNotificationCenter's observer token has no thread affinity. Its
// callback only copies immutable domain flags into a thread-safe watch channel.
unsafe impl Send for Observer {}

impl Observer {
    pub fn new() -> Self {
        let (sender, receiver) = watch::channel(None);
        let changes = sender.clone();
        let generation = Arc::new(Mutex::new(0));
        let notices = generation.clone();
        let block = RcBlock::new(move |_: NonNull<NSNotification>| read_domains(changes.clone(), notices.clone()));
        let supported = system_supports_file_provider();
        let domain_name = supported
            .then(|| notification_name(c"NSFileProviderDomainDidChange"))
            .flatten();
        // SAFETY: framework-provided notification name, no queue affinity; the
        // block captures only a sendable watch sender. Register before the read
        // that enables domain notifications so changes cannot be missed. A
        // system without the notification simply installs no observer.
        let token = domain_name.map(|name| unsafe {
            NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
                Some(name),
                None,
                None,
                &block,
            )
        });
        let cache_block = RcBlock::new(move |_: NonNull<NSNotification>| {
            if let Some(root) = super::CONTAINER.get() {
                crate::cache::changed(root);
            }
        });
        let materialized_name = supported
            .then(|| notification_name(c"NSFileProviderMaterializedSetDidChange"))
            .flatten();
        // SAFETY: framework-provided notification name; the block only accesses
        // thread-safe cache state and has no queue affinity. The center copies it.
        let cache_token = materialized_name.map(|name| unsafe {
            NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
                Some(name),
                None,
                None,
                &cache_block,
            )
        });
        if supported {
            read_domains(sender.clone(), generation.clone());
        } else {
            sender.send_replace(Some(Err("File Provider requires macOS 11 or later".into())));
        }
        Self {
            cache_token,
            token,
            receiver,
            sender,
            generation,
        }
    }

    /// Explicit demand/retry can recover an initial failed query even if the
    /// framework had not yet enabled change notifications. No periodic retry.
    pub fn refresh(&self) {
        read_domains(self.sender.clone(), self.generation.clone());
    }

    pub async fn changed(&mut self) -> Result<Status, watch::error::RecvError> {
        loop {
            self.receiver.changed().await?;
            if let Some(status) = self.receiver.borrow_and_update().clone() {
                return Ok(status);
            }
        }
    }
}

impl Default for Observer {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        // SAFETY: token was returned by this notification center. In-flight
        // callbacks retain their sender and never borrow the Observer.
        let center = NSNotificationCenter::defaultCenter();
        unsafe {
            if let Some(token) = &self.token {
                center.removeObserver(AsRef::<AnyObject>::as_ref(&**token));
            }
            if let Some(token) = &self.cache_token {
                center.removeObserver(AsRef::<AnyObject>::as_ref(&**token));
            }
        }
    }
}

fn read_domains(sender: watch::Sender<Option<Status>>, generation: Arc<Mutex<u64>>) {
    if !system_supports_file_provider() {
        sender.send_replace(Some(Err("File Provider requires macOS 11 or later".into())));
        return;
    }
    let revision = {
        let mut latest = generation.lock().unwrap();
        *latest += 1;
        *latest
    };
    let Some(class) = AnyClass::get(c"NSFileProviderManager") else {
        tracing::warn!(
            operation = "get_domains",
            cause = "manager_class_unavailable",
            "File Provider observation failed"
        );
        sender.send_replace(Some(Err("File Provider manager unavailable".into())));
        return;
    };
    let completion = RcBlock::new(move |domains: *mut NSArray<AnyObject>, error: *mut NSError| {
        // SAFETY: FileProvider owns these callback arguments through this call.
        // Only immutable documented properties are read; no manager instance,
        // domain registration or user-visible URL operation is performed.
        let status = unsafe {
            if let Some(error) = error.as_ref() {
                Err(format!("get_domains: {}", super::diagnostics::native_error(error)))
            } else if let Some(domains) = domains.as_ref() {
                Ok(domains.iter().find_map(|domain| {
                    let id: Retained<NSString> = msg_send![&*domain, identifier];
                    if id.to_string() != "oink-files" {
                        return None;
                    }
                    let hidden: Bool = msg_send![&*domain, isHidden];
                    let enabled: Bool = msg_send![&*domain, userEnabled];
                    let disconnected: Bool = msg_send![&*domain, isDisconnected];
                    Some(DomainStatus {
                        id: id.to_string(),
                        hidden: hidden.as_bool(),
                        user_enabled: enabled.as_bool(),
                        disconnected: disconnected.as_bool(),
                    })
                }))
            } else {
                Err("File Provider returned no domains".into())
            }
        };
        // A newer notification may have started a query while this one was in
        // flight. Its older callback must not overwrite the newer domain flags.
        let latest = generation.lock().unwrap();
        if *latest == revision {
            if let Err(cause) = &status {
                tracing::warn!(operation = "get_domains", %cause, "File Provider observation failed");
            }
            sender.send_replace(Some(status));
        }
    });
    // SAFETY: documented class method, correct callback ABI. FileProvider
    // copies the completion block and owns its asynchronous lifetime.
    unsafe {
        let _: () = msg_send![class, getDomainsWithCompletionHandler: &*completion];
    }
}
