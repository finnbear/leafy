use crate::AttrValue;
use std::borrow::Cow;
use std::fmt;
use std::rc::Rc;

/// A space-separated list of CSS classes.
///
/// A single `&'static str` class costs no allocation.
#[derive(Clone, Default)]
pub struct Classes {
    s: Option<AttrValue>,
    /// On the server, the stylesheets of its [`css!`](crate::css) classes,
    /// which go wherever the classes do (to the page that renders them).
    /// (Boxed, so the classes are no bigger without.)
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(clippy::box_collection)]
    pub(crate) css: Option<Box<Vec<&'static crate::Css>>>,
}

impl PartialEq for Classes {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.s == other.s
    }
}

impl Eq for Classes {}

impl Classes {
    #[inline]
    fn of(s: Option<AttrValue>) -> Self {
        Self {
            s,
            #[cfg(not(target_arch = "wasm32"))]
            css: None,
        }
    }

    /// No classes.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds class names (which may themselves be space separated).
    #[inline]
    pub fn push<T: Into<Classes>>(&mut self, class: T) {
        self.push_classes(class.into());
    }

    #[inline(never)]
    fn push_classes(&mut self, other: Classes) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(css) = &other.css {
            crate::css::merge(self.css.get_or_insert_default(), css);
        }
        match (&self.s, other.s) {
            (_, None) => {}
            (None, s) => self.s = s,
            (Some(a), Some(b)) => {
                let mut s = String::with_capacity(a.len() + 1 + b.len());
                s.push_str(a);
                s.push(' ');
                s.push_str(&b);
                self.s = Some(AttrValue::from(s));
            }
        }
    }

    #[inline(never)]
    fn from_static(s: &'static str) -> Self {
        let s = s.trim();
        Self::of((!s.is_empty()).then_some(AttrValue::Static(s)))
    }

    #[inline(never)]
    fn from_attr(s: AttrValue) -> Self {
        if s.trim().len() == s.len() {
            Self::of((!s.is_empty()).then_some(s))
        } else {
            Self::from_str(&s)
        }
    }

    #[inline(never)]
    fn from_str(s: &str) -> Self {
        let s = s.trim();
        Self::of((!s.is_empty()).then(|| AttrValue::from(s.to_owned())))
    }

    /// Whether `class` is one of the classes.
    pub fn contains<T: AsRef<str>>(&self, class: T) -> bool {
        let class = class.as_ref();
        self.as_str().split(' ').any(|c| c == class)
    }

    /// Whether there are no classes.
    pub fn is_empty(&self) -> bool {
        self.s.is_none()
    }

    /// As [`push`](Self::push).
    pub fn extend<T: Into<Classes>>(&mut self, other: T) {
        self.push(other);
    }

    /// The classes, space separated.
    pub fn as_str(&self) -> &str {
        self.s.as_deref().unwrap_or("")
    }

    #[doc(hidden)]
    #[inline]
    pub fn into_attr(self) -> Option<AttrValue> {
        self.s
    }
}

impl fmt::Display for Classes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for Classes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl From<&'static str> for Classes {
    #[inline]
    fn from(s: &'static str) -> Self {
        Self::from_static(s)
    }
}

impl From<&String> for Classes {
    #[inline]
    fn from(s: &String) -> Self {
        Self::from_str(s)
    }
}

impl From<String> for Classes {
    #[inline]
    fn from(s: String) -> Self {
        Self::from_attr(AttrValue::from(s))
    }
}

impl From<Rc<str>> for Classes {
    #[inline]
    fn from(s: Rc<str>) -> Self {
        Self::from_attr(AttrValue::Rc(s))
    }
}

impl From<Cow<'static, str>> for Classes {
    #[inline]
    fn from(s: Cow<'static, str>) -> Self {
        Self::from_attr(AttrValue::from(s))
    }
}

impl From<AttrValue> for Classes {
    #[inline]
    fn from(s: AttrValue) -> Self {
        Self::from_attr(s)
    }
}

impl From<&AttrValue> for Classes {
    #[inline]
    fn from(s: &AttrValue) -> Self {
        Self::from_attr(s.clone())
    }
}

impl From<&Classes> for Classes {
    #[inline]
    fn from(s: &Classes) -> Self {
        s.clone()
    }
}

impl<T: Into<Classes>> From<Option<T>> for Classes {
    #[inline]
    fn from(s: Option<T>) -> Self {
        match s {
            Some(s) => s.into(),
            None => Self::new(),
        }
    }
}

impl<T: Into<Classes> + Clone> From<&Option<T>> for Classes {
    #[inline]
    fn from(s: &Option<T>) -> Self {
        s.clone().into()
    }
}

impl<T: Into<Classes>> From<Vec<T>> for Classes {
    fn from(v: Vec<T>) -> Self {
        v.into_iter().collect()
    }
}

impl<T: Into<Classes> + Clone> From<&[T]> for Classes {
    fn from(v: &[T]) -> Self {
        v.iter().cloned().collect()
    }
}

impl<T: Into<Classes>, const N: usize> From<[T; N]> for Classes {
    fn from(v: [T; N]) -> Self {
        v.into_iter().collect()
    }
}

impl<T: Into<Classes>> FromIterator<T> for Classes {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut ret = Self::new();
        for c in iter {
            ret.push(c);
        }
        ret
    }
}

impl From<Classes> for AttrValue {
    fn from(c: Classes) -> Self {
        c.s.unwrap_or_default()
    }
}

impl From<&Classes> for AttrValue {
    fn from(c: &Classes) -> Self {
        c.s.clone().unwrap_or_default()
    }
}

impl From<Classes> for String {
    fn from(c: Classes) -> Self {
        c.as_str().to_owned()
    }
}
