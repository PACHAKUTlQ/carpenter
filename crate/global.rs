use std::cell::RefCell;
use crate::uri_match::HostnameMatchTree;


pub struct GlobalStuff {
    pub dns_cache: HostnameMatchTree<std::net::IpAddr>,
    pub cnt: usize,
}


static mut glb_stuff_ptr: *mut RefCell<GlobalStuff> = 0 as *mut _;


pub fn init_global_stuff() {
    let bb = Box::new(RefCell::new(GlobalStuff {
        dns_cache: HostnameMatchTree::new(),
        cnt: 0,
    }));
    unsafe {
        glb_stuff_ptr = Box::leak(bb) as *mut _;
    };
}


pub fn get_global_stuff() -> &'static mut RefCell<GlobalStuff> {
    unsafe { &mut *glb_stuff_ptr }
}

