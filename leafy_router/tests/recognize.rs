//! `#[derive(Routable)]`'s `recognize` matches as yew-router 0.20's does
//! (route-recognizer, then `from_path`).

use leafy_router::Routable;

#[derive(Debug, Clone, PartialEq, Routable)]
enum Route {
    #[at("/")]
    Home,
    #[at("/posts")]
    Posts,
    #[at("/posts/new")]
    NewPost,
    #[at("/posts/:id")]
    Post { id: u32 },
    #[at("/posts/:id/edit/")]
    EditPost { id: u32 },
    #[at("/users/:name")]
    User { name: String },
    #[at("/users/:name/posts/:id")]
    UserPost { name: String, id: u32 },
    #[at("/files/*path")]
    File { path: String },
    #[at("/a/*")]
    AnyA,
    #[at("/:lang/about")]
    About { lang: String },
    #[at("/v.:ver")]
    Version { ver: u32 },
    #[at("/x/:a.:b")]
    Dotted { a: String, b: String },
    #[not_found]
    #[at("/404")]
    NotFound,
}

/// yew-router 0.20's `recognize`.
fn reference<R: Routable>(pathname: &str) -> Option<R> {
    let mut router = route_recognizer::Router::new();
    for path in R::routes() {
        router.add(path.strip_suffix('/').unwrap_or(path), path.to_string());
    }
    let pathname = pathname.strip_suffix('/').unwrap_or(pathname);
    match router.recognize(pathname) {
        Ok(m) => {
            let params = m.params().into_iter().collect();
            R::from_path(m.handler(), &params).or_else(R::not_found_route)
        }
        Err(_) => R::not_found_route(),
    }
}

#[test]
fn matches_yew_router() {
    let words = [
        "", "posts", "users", "files", "a", "v", "x", "new", "edit", "about", "1", "42", "-3",
        "bob", "x.y", "v.7", "a%20b", "%2F", "é", "404", "posts.1",
    ];
    let mut paths = vec![String::new(), "/".to_owned()];
    for a in words {
        for b in words {
            for c in ["", "edit", "posts", "7", "z"] {
                for slash in ["", "/"] {
                    let mut p = format!("/{a}/{b}");
                    if !c.is_empty() {
                        p.push('/');
                        p.push_str(c);
                    }
                    p.push_str(slash);
                    paths.push(p);
                }
            }
        }
        paths.push(format!("/{a}"));
        paths.push(format!("/{a}/"));
    }
    let mut differences = Vec::new();
    for p in &paths {
        let (ours, theirs) = (Route::recognize(p), reference::<Route>(p));
        if ours != theirs {
            differences.push(format!("{p:?}: {ours:?} != {theirs:?}"));
        }
    }
    assert!(
        differences.is_empty(),
        "{} of {} differ:\n{}",
        differences.len(),
        paths.len(),
        differences.join("\n")
    );
}

#[test]
fn to_path_round_trips() {
    for r in [
        Route::Home,
        Route::Post { id: 5 },
        Route::User {
            name: "a b/c".into(),
        },
        Route::File {
            path: "dir/f i.txt".into(),
        },
        Route::Version { ver: 2 },
    ] {
        assert_eq!(
            Route::recognize(&r.to_path()),
            Some(r.clone()),
            "{}",
            r.to_path()
        );
    }
}

/// A `Routable` wrapping others, chaining their routes.
#[derive(Debug, Clone, PartialEq)]
enum Nested<G> {
    Inner(Route),
    Game(G),
}

impl<G: Routable> Routable for Nested<G> {
    type Routes = std::iter::Chain<G::Routes, <Route as Routable>::Routes>;

    fn from_path(path: &str, params: &std::collections::HashMap<&str, &str>) -> Option<Self> {
        G::from_path(path, params)
            .map(Self::Game)
            .or_else(|| Route::from_path(path, params).map(Self::Inner))
    }
    fn to_path(&self) -> String {
        match self {
            Self::Inner(r) => r.to_path(),
            Self::Game(g) => g.to_path(),
        }
    }
    fn routes() -> Self::Routes {
        G::routes().chain(Route::routes())
    }
    fn not_found_route() -> Option<Self> {
        None
    }
    fn recognize(pathname: &str) -> Option<Self> {
        G::recognize(pathname).map(Self::Game)
    }
}

#[test]
fn nested_routes() {
    assert_eq!(Nested::<Route>::routes().count(), 26);
}

#[test]
fn routes_listed() {
    assert_eq!(Route::routes().count(), 13);
    assert_eq!(Route::routes().next(), Some("/"));
}

#[derive(Debug, Clone, PartialEq, Routable)]
enum Wild {
    #[at("/*a/x/*b/y/*c/z")]
    Three { a: String, b: String, c: String },
    #[not_found]
    #[at("/404")]
    NotFound,
}

/// Backtracking is bounded: a long path that almost matches a route with
/// several wildcards doesn't take long.
#[test]
fn pathological_path_is_fast() {
    let path = format!("/{}", "x/y/".repeat(2000));
    let start = std::time::Instant::now();
    assert_eq!(Wild::recognize(&path), Some(Wild::NotFound));
    assert!(start.elapsed().as_millis() < 500, "{:?}", start.elapsed());
    assert_eq!(
        Wild::recognize("/a/x/b/y/c/z"),
        Some(Wild::Three {
            a: "a".into(),
            b: "b".into(),
            c: "c".into()
        })
    );
}
