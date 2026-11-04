//! [`Callback`]: a reference-counted function, compared by identity.
//!
//! Not `Rc<dyn Fn>`: a `dyn Fn` vtable has six entries, including a
//! `call_once` shim the compiler generates for every closure type. Here each
//! closure type costs one `call` function (its body) and, only if it owns
//! something to drop, one `drop` function. Reference counting is
//! non-generic.

use std::alloc::{Layout, alloc, dealloc, handle_alloc_error};
use std::cell::Cell;
use std::fmt;
use std::marker::PhantomData;
use std::ptr::{self, NonNull};

/// The start of every callback allocation.
#[repr(C)]
struct Header {
    strong: Cell<usize>,
    size: usize,
    align: usize,
    /// Drops the function in place, if it needs dropping.
    drop: Option<unsafe fn(*mut Header)>,
}

/// A header followed by the function pointer to call.
#[repr(C)]
struct Prefix<IN, OUT> {
    header: Header,
    call: unsafe fn(*const Header, IN) -> OUT,
}

/// A whole allocation.
#[repr(C)]
struct CbBox<IN, OUT, F> {
    prefix: Prefix<IN, OUT>,
    f: F,
}

unsafe fn call<IN, OUT, F: Fn(IN) -> OUT>(p: *const Header, input: IN) -> OUT {
    unsafe { ((*(p as *const CbBox<IN, OUT, F>)).f)(input) }
}

unsafe fn drop_f<IN, OUT, F>(p: *mut Header) {
    unsafe { ptr::drop_in_place(&mut (*(p as *mut CbBox<IN, OUT, F>)).f) }
}

#[inline(never)]
fn alloc_box(layout: Layout) -> NonNull<Header> {
    // SAFETY: the layout has nonzero size (it contains a header).
    let p = unsafe { alloc(layout) };
    match NonNull::new(p as *mut Header) {
        Some(p) => p,
        None => handle_alloc_error(layout),
    }
}

#[inline(never)]
fn inc(p: NonNull<Header>) {
    // SAFETY: `p` is live.
    let strong = unsafe { &p.as_ref().strong };
    let n = strong.get().wrapping_add(1);
    if n == 0 {
        // As `Rc`: the count overflowed (clones were leaked). In wasm, a
        // trap, without panicking.
        std::process::abort();
    }
    strong.set(n);
}

#[inline(never)]
fn dec(p: NonNull<Header>) {
    // SAFETY: `p` is live until the count reaches zero.
    unsafe {
        let header = p.as_ref();
        let strong = header.strong.get() - 1;
        header.strong.set(strong);
        if strong == 0 {
            let layout = Layout::from_size_align_unchecked(header.size, header.align);
            if let Some(drop) = header.drop {
                drop(p.as_ptr());
            }
            dealloc(p.as_ptr() as *mut u8, layout);
        }
    }
}

/// A reference-counted function, compared by identity.
pub struct Callback<IN, OUT = ()> {
    ptr: NonNull<Header>,
    #[allow(clippy::type_complexity)]
    _marker: PhantomData<(*const (), fn(IN) -> OUT)>,
}

impl<IN, OUT, F: Fn(IN) -> OUT + 'static> From<F> for Callback<IN, OUT> {
    #[inline]
    fn from(f: F) -> Self {
        let layout = Layout::new::<CbBox<IN, OUT, F>>();
        let ptr = alloc_box(layout);
        // SAFETY: `ptr` is a fresh allocation of `layout`.
        unsafe {
            ptr::write(
                ptr.as_ptr() as *mut CbBox<IN, OUT, F>,
                CbBox {
                    prefix: Prefix {
                        header: Header {
                            strong: Cell::new(1),
                            size: layout.size(),
                            align: layout.align(),
                            drop: if std::mem::needs_drop::<F>() {
                                Some(drop_f::<IN, OUT, F>)
                            } else {
                                None
                            },
                        },
                        call: call::<IN, OUT, F>,
                    },
                    f,
                },
            );
        }
        Self {
            ptr,
            _marker: PhantomData,
        }
    }
}

impl<IN, OUT> Clone for Callback<IN, OUT> {
    #[inline]
    fn clone(&self) -> Self {
        inc(self.ptr);
        Self {
            ptr: self.ptr,
            _marker: PhantomData,
        }
    }
}

impl<IN, OUT> Drop for Callback<IN, OUT> {
    #[inline]
    fn drop(&mut self) {
        dec(self.ptr);
    }
}

impl<IN, OUT> PartialEq for Callback<IN, OUT> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.ptr == other.ptr
    }
}

impl<IN, OUT> fmt::Debug for Callback<IN, OUT> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Callback<_>")
    }
}

impl<IN, OUT> Callback<IN, OUT> {
    /// Calls the callback.
    #[inline]
    pub fn emit(&self, value: IN) -> OUT {
        // SAFETY: every allocation starts with a `Prefix<IN, OUT>`.
        unsafe {
            let prefix = self.ptr.as_ptr() as *const Prefix<IN, OUT>;
            ((*prefix).call)(self.ptr.as_ptr(), value)
        }
    }

    /// A callback that calls this one with the input mapped through `f`.
    pub fn reform<F, T>(&self, f: F) -> Callback<T, OUT>
    where
        F: Fn(T) -> IN + 'static,
        IN: 'static,
        OUT: 'static,
    {
        let this = self.clone();
        Callback::from(move |t| this.emit(f(t)))
    }

    /// A callback that calls this one only if `f` returns `Some`.
    pub fn filter_reform<F, T>(&self, f: F) -> Callback<T, Option<OUT>>
    where
        F: Fn(T) -> Option<IN> + 'static,
        IN: 'static,
        OUT: 'static,
    {
        let this = self.clone();
        Callback::from(move |t| f(t).map(|i| this.emit(i)))
    }

    /// Reinterprets the input type.
    ///
    /// # Safety
    /// `IN` and `IN2` must be ABI-compatible (e.g. `#[repr(transparent)]`
    /// wrappers of one another).
    #[doc(hidden)]
    pub unsafe fn __cast<IN2>(self) -> Callback<IN2, OUT> {
        let ptr = self.ptr;
        std::mem::forget(self);
        Callback {
            ptr,
            _marker: PhantomData,
        }
    }
}

impl<IN: 'static, OUT: Default + 'static> Callback<IN, OUT> {
    /// A callback that does nothing.
    pub fn noop() -> Self {
        Self::from(|_| OUT::default())
    }
}

impl<IN: 'static, OUT: Default + 'static> Default for Callback<IN, OUT> {
    fn default() -> Self {
        Self::noop()
    }
}
