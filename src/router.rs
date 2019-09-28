use crate::endpoint::AsyncResponse;
use futures::future::{Future, FutureExt};
use http::method::Method;
use http_service::{Request, Response};
use std::sync::Arc;
use std::{collections::HashMap, pin::Pin};

pub(crate) type RouteFn = Box<dyn Fn(Request) -> AsyncResponse + Send + Sync>;

#[derive(Debug)]
pub struct StaticSegment {
    pub value: &'static str,
    pub position: usize,
}

#[derive(Debug)]
pub struct DynamicSegment {
    pub position: usize,
}

pub struct Route {
    pub static_segments: Vec<StaticSegment>,
    pub dynamic_segments: Vec<DynamicSegment>,
    pub handler: RouteFn,
}

pub struct Router {
    table: HashMap<Method, Vec<Route>>,
}

impl Router {
    pub fn new() -> Self {
        Router {
            table: HashMap::new(),
        }
    }

    pub fn add(&mut self, method: Method, route: impl Fn() -> Route) {
        self.table
            .entry(method)
            .or_insert(vec![route()])
            .push(route());
    }

    pub(crate) fn lookup(
        self: Arc<Self>,
        req: Request,
    ) -> Pin<Box<dyn Future<Output = Result<Response, std::io::Error>> + Send>> {
        let method = req.method();
        let raw_route = RawRoute::from_path(req.uri().path().into());
        let maybe_route = if let Some(routes) = self.table.get(method) {
            routes
                .iter()
                .filter(|route| paths_match(route, &raw_route))
                .nth(0)
        } else {
            return not_found().boxed();
        };

        if let Some(route) = maybe_route {
            route.dynamic_segments.iter().for_each(|dynamic_segment| {
                dbg!(&raw_route.raw_segments[dynamic_segment.position].value);
            });
            return (route.handler)(req).boxed();
        }

        not_found().boxed()
    }
}

fn paths_match(route: &Route, raw_route: &RawRoute) -> bool {
    if raw_route.raw_segments.len() == route.static_segments.len() + route.dynamic_segments.len() {
        let static_matches = route
            .static_segments
            .iter()
            .fold(true, |is_match, static_segment| {
                let raw_segment = &raw_route.raw_segments[static_segment.position];
                is_match & (raw_segment == static_segment)
            });

        let dynamic_matches =
            route
                .dynamic_segments
                .iter()
                .fold(true, |is_match, dynamic_segment| {
                    let raw_segment = &raw_route.raw_segments[dynamic_segment.position];
                    is_match & (raw_segment == dynamic_segment)
                });

        static_matches & dynamic_matches
    } else {
        false
    }
}

async fn not_found() -> Result<Response, std::io::Error> {
    use crate::endpoint::error_response;
    use http::status::StatusCode;
    use serde_json::json;

    error_response(json!("not found"), StatusCode::NOT_FOUND)
}

#[derive(Debug)]
pub(crate) struct RawSegment<'s> {
    value: &'s str,
    position: usize,
}

#[derive(Debug)]
pub(crate) struct RawRoute<'s> {
    pub raw_segments: Vec<RawSegment<'s>>,
}

impl<'s> RawRoute<'s> {
    pub(crate) fn from_path(path: &'s str) -> Self {
        let raw_segments = {
            let mut segments = vec![];
            let mut split = path.split("/");
            let _ = split.next(); // hack
            split.enumerate().for_each(|(i, segment)| {
                segments.push(RawSegment {
                    value: segment,
                    position: i,
                });
            });
            segments
        };

        Self { raw_segments }
    }
}

impl<'s> PartialEq<RawSegment<'s>> for StaticSegment {
    fn eq(&self, other: &RawSegment) -> bool {
        self.position == other.position && self.value == other.value
    }
}

impl<'s> PartialEq<RawSegment<'s>> for DynamicSegment {
    fn eq(&self, other: &RawSegment) -> bool {
        self.position == other.position
    }
}

impl<'s> PartialEq<StaticSegment> for RawSegment<'s> {
    fn eq(&self, other: &StaticSegment) -> bool {
        other == self
    }
}

impl<'s> PartialEq<DynamicSegment> for RawSegment<'s> {
    fn eq(&self, other: &DynamicSegment) -> bool {
        other == self
    }
}
