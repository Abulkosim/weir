use crate::core::network::Path;
use block2::RcBlock;
use std::ffi::{c_char, c_int, c_void};

type Object = *mut c_void;

const STATUS_SATISFIED: c_int = 1;
const INTERFACE_WIFI: c_int = 1;
const INTERFACE_CELLULAR: c_int = 2;
const INTERFACE_WIRED: c_int = 3;

#[link(name = "Network", kind = "framework")]
extern "C" {
    fn nw_path_monitor_create() -> Object;
    fn nw_path_monitor_set_queue(monitor: Object, queue: Object);
    fn nw_path_monitor_set_update_handler(monitor: Object, handler: &block2::Block<dyn Fn(Object)>);
    fn nw_path_monitor_start(monitor: Object);
    fn nw_path_get_status(path: Object) -> c_int;
    fn nw_path_is_expensive(path: Object) -> bool;
    fn nw_path_is_constrained(path: Object) -> bool;
    fn nw_path_uses_interface_type(path: Object, interface: c_int) -> bool;
}

extern "C" {
    fn dispatch_queue_create(label: *const c_char, attributes: Object) -> Object;
}

/// Reads one `nw_path_t`. Safety: `path` is the live path passed to the update handler.
unsafe fn read(path: Object) -> Path {
    Path {
        online: nw_path_get_status(path) == STATUS_SATISFIED,
        expensive: nw_path_is_expensive(path),
        constrained: nw_path_is_constrained(path),
        cost: None,
        roaming: false,
        over_limit: false,
        wifi: nw_path_uses_interface_type(path, INTERFACE_WIFI),
        wired: nw_path_uses_interface_type(path, INTERFACE_WIRED),
        cellular: nw_path_uses_interface_type(path, INTERFACE_CELLULAR),
    }
}

pub fn watch(on_change: impl Fn(Path) + Send + Sync + 'static) {
    let handler = RcBlock::new(move |path: Object| {
        if !path.is_null() {
            // Network.framework passes a valid path for the duration of the call.
            on_change(unsafe { read(path) });
        }
    });
    // The monitor and queue live for the whole app; Network.framework copies the block.
    unsafe {
        let queue =
            dispatch_queue_create(c"com.abulkosim.weir.path".as_ptr(), std::ptr::null_mut());
        let monitor = nw_path_monitor_create();
        nw_path_monitor_set_queue(monitor, queue);
        nw_path_monitor_set_update_handler(monitor, &handler);
        nw_path_monitor_start(monitor);
    }
}
