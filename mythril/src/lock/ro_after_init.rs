use core::{cell::UnsafeCell, ops::Deref};

pub struct RoAfterInit<T> {
    data: UnsafeCell<Option<T>>,
}

impl<T> RoAfterInit<T> {
    pub const fn uninitialized() -> Self {
        RoAfterInit {
            data: UnsafeCell::new(None),
        }
    }

    pub unsafe fn init(this: &Self, val: T) {
        *this.data.get() = Some(val);
    }

    pub fn is_initialized(this: &Self) -> bool {
        unsafe { &*this.data.get() }.is_some()
    }
}

// BspOnce is Send/Sync because the contents are immutable after init
unsafe impl<T: Send> Send for RoAfterInit<T> {}
unsafe impl<T: Send + Sync> Sync for RoAfterInit<T> {}

impl<T> Deref for RoAfterInit<T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe {
            (*self.data.get())
                .as_ref()
                .expect("Attempt to use BspOnce before init")
        }
    }
}
