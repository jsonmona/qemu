use std::fmt::Debug;
use std::ops::Deref;

use crate::fifo_processor::cmd::FifoCmd;

pub enum RefOrBox<'a, T: ?Sized> {
    Refed(&'a T),
    Boxed(Box<T>),
}

unsafe impl<'a, T: Sync + ?Sized> Send for RefOrBox<'a, T> {}

impl<'a, T: ?Sized> RefOrBox<'a, T> {
    pub fn from_ref(value: &'a T) -> Self {
        Self::Refed(value)
    }

    pub fn from_box(value: Box<T>) -> Self {
        Self::Boxed(value)
    }

    pub fn is_ref(&self) -> bool {
        match self {
            Self::Refed(_) => true,
            Self::Boxed(_) => false,
        }
    }

    pub fn is_box(&self) -> bool {
        match self {
            Self::Refed(_) => false,
            Self::Boxed(_) => true,
        }
    }
}

impl<'a, T: ?Sized + PartialEq> PartialEq for RefOrBox<'a, T> {
    fn eq(&self, other: &Self) -> bool {
        T::eq(self.deref(), other.deref())
    }

    fn ne(&self, other: &Self) -> bool {
        T::ne(self.deref(), other.deref())
    }
}

impl<'a, T: ?Sized> Deref for RefOrBox<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Refed(x) => x,
            Self::Boxed(x) => x,
        }
    }
}

impl<'a, T: ?Sized + Debug> Debug for RefOrBox<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refed(arg0) => f.debug_tuple("Refed").field(arg0).finish(),
            Self::Boxed(arg0) => f.debug_tuple("Boxed").field(arg0).finish(),
        }
    }
}

impl<'a, T: ?Sized> From<&'a T> for RefOrBox<'a, T> {
    fn from(value: &'a T) -> Self {
        Self::Refed(value)
    }
}

impl<'a, T: ?Sized> From<Box<T>> for RefOrBox<'a, T> {
    fn from(value: Box<T>) -> Self {
        Self::Boxed(value)
    }
}

// Needs coerce_unsized #18598 to make these generic
impl<'a, U: FifoCmd + 'static> From<&'a U> for RefOrBox<'a, dyn FifoCmd> {
    fn from(value: &'a U) -> Self {
        Self::Refed(value)
    }
}

impl<'a, U: FifoCmd + 'static> From<Box<U>> for RefOrBox<'a, dyn FifoCmd> {
    fn from(value: Box<U>) -> Self {
        Self::Boxed(value)
    }
}
