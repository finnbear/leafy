//! What `#[derive(Routable)]` expands to calls.
//!
//! Routes are matched as yew-router 0.20 matches them (with
//! `route-recognizer`), without building a router at runtime: the derive
//! compiles each route into literal text and captures, sorted so the first
//! that matches is the one `route-recognizer` would pick (the most static
//! segments, then the most `:params`, then the most `*wildcards`; the first
//! declared of equals).

pub use urlencoding::{decode as decode_for_url, encode as encode_for_url};

/// Part of a compiled route.
pub enum Part {
    /// Literal text (including separators).
    Lit(&'static str),
    /// `:name`: one or more characters, other than `/`.
    Param,
    /// `*` or `*name`: one or more characters.
    Wild,
}

pub fn encode_path_for_url(path: &str) -> String {
    path.split('/')
        .map(encode_for_url)
        .collect::<Vec<_>>()
        .join("/")
}

/// The index of the first of `routes` that matches `pathname`, with its
/// captures (in order) in `caps`.
#[inline(never)]
pub fn recognize<'a>(routes: &[&[Part]], pathname: &'a str, caps: &mut [&'a str]) -> Option<usize> {
    let path = pathname.strip_prefix('/').unwrap_or(pathname);
    let path = path.strip_suffix('/').unwrap_or(path);
    routes.iter().position(|parts| {
        // Backtracking over several wildcards is polynomial in the path's
        // length: bound the work (a route not matched within it doesn't
        // match; real paths take a few steps).
        let mut budget = 1 << 16;
        matches(parts, path, caps, &mut budget)
    })
}

fn matches<'a>(parts: &[Part], path: &'a str, caps: &mut [&'a str], budget: &mut u32) -> bool {
    if *budget == 0 {
        return false;
    }
    *budget -= 1;
    let Some((part, rest)) = parts.split_first() else {
        return path.is_empty();
    };
    let max = match part {
        Part::Lit(lit) => {
            return path
                .strip_prefix(lit)
                .is_some_and(|path| matches(rest, path, caps, budget));
        }
        Part::Param => path.find('/').unwrap_or(path.len()),
        Part::Wild => path.len(),
    };
    // At least one character; the longest that lets the rest match.
    let (cap, caps_rest) = caps.split_first_mut().unwrap();
    for end in (1..=max).rev() {
        if path.is_char_boundary(end) && matches(rest, &path[end..], caps_rest, budget) {
            *cap = &path[..end];
            return true;
        }
    }
    false
}

/// A captured parameter, URL decoded and parsed.
#[inline]
pub fn param<T: std::str::FromStr>(capture: &str) -> Option<T> {
    decode_for_url(capture).ok()?.parse().ok()
}
