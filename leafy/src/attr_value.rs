use std::borrow::Cow;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::rc::Rc;

/// An immutable string that is cheap to clone: either `'static` or
/// reference counted. Equality is by contents.
pub enum AttrValue {
    /// A `'static` string.
    Static(&'static str),
    /// A reference-counted string.
    Rc(Rc<str>),
}

impl Clone for AttrValue {
    #[inline(never)]
    fn clone(&self) -> Self {
        match self {
            Self::Static(s) => Self::Static(s),
            Self::Rc(s) => Self::Rc(s.clone()),
        }
    }
}

impl AttrValue {
    /// The string.
    #[inline]
    pub fn as_str(&self) -> &str {
        self
    }
}

impl Default for AttrValue {
    #[inline]
    fn default() -> Self {
        Self::Static("")
    }
}

impl Deref for AttrValue {
    type Target = str;
    #[inline]
    fn deref(&self) -> &str {
        match self {
            Self::Static(s) => s,
            Self::Rc(s) => s,
        }
    }
}

impl AsRef<str> for AttrValue {
    #[inline]
    fn as_ref(&self) -> &str {
        self
    }
}

impl std::borrow::Borrow<str> for AttrValue {
    #[inline]
    fn borrow(&self) -> &str {
        self
    }
}

impl PartialEq for AttrValue {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}
impl Eq for AttrValue {}

impl PartialEq<str> for AttrValue {
    fn eq(&self, other: &str) -> bool {
        &**self == other
    }
}
impl PartialEq<&str> for AttrValue {
    fn eq(&self, other: &&str) -> bool {
        &**self == *other
    }
}
impl PartialEq<String> for AttrValue {
    fn eq(&self, other: &String) -> bool {
        **self == **other
    }
}

impl PartialOrd for AttrValue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for AttrValue {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (**self).cmp(&**other)
    }
}

impl Hash for AttrValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (**self).hash(state)
    }
}

impl fmt::Display for AttrValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self)
    }
}

impl fmt::Debug for AttrValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl From<&'static str> for AttrValue {
    #[inline]
    fn from(s: &'static str) -> Self {
        Self::Static(s)
    }
}

impl From<String> for AttrValue {
    #[inline(never)]
    fn from(s: String) -> Self {
        Self::Rc(Rc::from(s))
    }
}

impl From<&String> for AttrValue {
    #[inline(never)]
    fn from(s: &String) -> Self {
        Self::Rc(Rc::from(s.as_str()))
    }
}

impl From<Rc<str>> for AttrValue {
    #[inline]
    fn from(s: Rc<str>) -> Self {
        Self::Rc(s)
    }
}

impl From<Cow<'static, str>> for AttrValue {
    #[inline(never)]
    fn from(s: Cow<'static, str>) -> Self {
        match s {
            Cow::Borrowed(s) => Self::Static(s),
            Cow::Owned(s) => s.into(),
        }
    }
}

impl From<&AttrValue> for AttrValue {
    #[inline]
    fn from(s: &AttrValue) -> Self {
        s.clone()
    }
}

impl From<AttrValue> for String {
    fn from(s: AttrValue) -> Self {
        s.to_string()
    }
}
