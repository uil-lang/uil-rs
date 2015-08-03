/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use freetype_sys::FTErrorMethods;
use freetype_sys::FT_Add_Default_Modules;
use freetype_sys::FT_Done_Library;
use freetype_sys::FT_Library;
use freetype_sys::FT_Memory;
use freetype_sys::FT_New_Library;
use freetype_sys::FT_MemoryRec;

use std::ptr;
use std::rc::Rc;
use std::rt::heap;

use libc::{c_void, c_long};
use util::heap_size_of;

// We pass a |User| struct -- via an opaque |void*| -- to FreeType each time a new instance is
// created. FreeType passes it back to the ft_alloc/ft_realloc/ft_free callbacks. We use it to
// record the memory usage of each FreeType instance.
struct User {
    size: usize,
}

// FreeType doesn't require any particular alignment for allocations.
const FT_ALIGNMENT: usize = 1;

extern fn ft_alloc(mem: FT_Memory, req_size: c_long) -> *mut c_void {
    unsafe {
        let ptr = heap::allocate(req_size as usize, FT_ALIGNMENT) as *mut c_void;
        let actual_size = heap_size_of(ptr);

        let user = (*mem).user as *mut User;
        (*user).size += actual_size;

        ptr
    }
}

extern fn ft_free(mem: FT_Memory, ptr: *mut c_void) {
    unsafe {
        let actual_size = heap_size_of(ptr);

        let user = (*mem).user as *mut User;
        (*user).size -= actual_size;

        heap::deallocate(ptr as *mut u8, actual_size, FT_ALIGNMENT);
    }
}

extern fn ft_realloc(mem: FT_Memory, _cur_size: c_long, new_req_size: c_long,
                     old_ptr: *mut c_void) -> *mut c_void {
    unsafe {
        let old_actual_size = heap_size_of(old_ptr);
        let new_ptr = heap::reallocate(old_ptr as *mut u8, old_actual_size,
                                       new_req_size as usize, FT_ALIGNMENT) as *mut c_void;
        let new_actual_size = heap_size_of(new_ptr);

        let user = (*mem).user as *mut User;
        (*user).size += new_actual_size - old_actual_size;

        new_ptr
    }
}

// A |*mut User| field in a struct triggers a "use of `#[derive]` with a raw pointer" warning from
// rustc. But using a typedef avoids this, so...
pub type UserPtr = *mut User;

// WARNING: We need to be careful how we use this struct. See the comment about Rc<> in
// FontContextHandle.
#[derive(Clone)]
pub struct FreeTypeLibraryHandle {
    pub ctx: FT_Library,
    mem: FT_Memory,
    user: UserPtr,
}

impl Drop for FreeTypeLibraryHandle {
    fn drop(&mut self) {
        assert!(!self.ctx.is_null());
        unsafe {
            FT_Done_Library(self.ctx);
            Box::from_raw(self.mem);
            Box::from_raw(self.user);
        }
    }
}

#[derive(Clone)]
pub struct FontContextHandle {
    // WARNING: FreeTypeLibraryHandle contains raw pointers, is clonable, and also implements
    // `Drop`. This field needs to be Rc<> to make sure that the `drop` function is only called
    // once, otherwise we'll get crashes. Yuk.
    pub ctx: Rc<FreeTypeLibraryHandle>,
}

impl FontContextHandle {
    pub fn new() -> FontContextHandle {
        let user = Box::into_raw(box User {
            size: 0,
        });
        let mem = Box::into_raw(box FT_MemoryRec {
            user: user as *mut c_void,
            alloc: ft_alloc,
            free: ft_free,
            realloc: ft_realloc,
        });
        unsafe {
            let mut ctx: FT_Library = ptr::null_mut();

            let result = FT_New_Library(mem, &mut ctx);
            if !result.succeeded() { panic!("Unable to initialize FreeType library"); }

            FT_Add_Default_Modules(ctx);

            FontContextHandle {
                ctx: Rc::new(FreeTypeLibraryHandle { ctx: ctx, mem: mem, user: user }),
            }
        }
    }
}
