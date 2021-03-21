// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Types and traits to navigate cursored collections.
//!
//! Much of this module can be considered an implementation detail; the main intended entry point
//! to this code is `CursorIter`, and that can just be used as a stream to ignore the rest of this
//! module. The rest of it is available to make sure consumers of the API can understand precisely
//! what types come out of functions that return `CursorIter`.

use std::fmt::Display;
use std::future::Future;
use std::num::NonZeroI64;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::Stream;
use serde::{de::DeserializeOwned, Deserialize};

use crate::common::*;
use crate::error::Result;
use crate::{auth, list, user};

///Trait to generalize over paginated views of API results.
///
///Types that implement Cursor are used as intermediate steps in [`CursorIter`][]'s Stream
///implementation, to properly load the data from Twitter. Most of the time you don't need to deal
///with Cursor structs directly, but you can get them via `CursorIter`'s manual paging
///functionality.
///
///[`CursorIter`]: struct.CursorIter.html
pub trait Cursor {
    ///What type is being returned by the API call?
    type Item;

    ///The type of the id used to track pages.
    ///
    ///This will usually be an `i64` or a similar numeric type.
    type Id: Display + Unpin + Send;

    ///Name of the parameter used in requests to indicate how many items to return in this page.
    ///
    ///The default should be fine for most endpoints; this item only exists in the first place
    ///because the Twitter V2 Search API uses a different parameter name.
    const COUNT_PARAMETER_NAME: &'static str = "count";

    ///Name of the parameter used in requests to indicate where to start the page that's returned.
    ///
    ///Like [`COUNT_PARAMETER_NAME`], this only exists becaue the Twitter V2 Search API has a
    ///different name for this than the other endpoints do.
    const STARTING_CURSOR_PARAMETER_NAME: &'static str = "cursor";

    ///Returns the id for the previous page of results, if available.
    fn previous_cursor_id(&self) -> Option<Self::Id>;
    ///Returns the id for the next next page of results, if available.
    fn next_cursor_id(&self) -> Option<Self::Id>;
    ///Unwraps the cursor, returning the collection of results from inside.
    fn into_inner(self) -> Vec<Self::Item>;
}

///Represents a single-page view into a list of users.
///
///This type is intended to be used in the background by [`CursorIter`][] to hold an intermediate
///list of users to iterate over. See that struct's documentation for details.
///
///[`CursorIter`]: struct.CursorIter.html
#[derive(Deserialize)]
pub struct UserCursor {
    ///Numeric reference to the previous page of results.
    pub previous_cursor: i64,
    ///Numeric reference to the next page of results.
    pub next_cursor: i64,
    ///The list of users in this page of results.
    pub users: Vec<user::TwitterUser>,
}

impl Cursor for UserCursor {
    type Item = user::TwitterUser;
    type Id = NonZeroI64;

    fn previous_cursor_id(&self) -> Option<NonZeroI64> {
        NonZeroI64::new(self.previous_cursor)
    }

    fn next_cursor_id(&self) -> Option<NonZeroI64> {
        NonZeroI64::new(self.next_cursor)
    }

    fn into_inner(self) -> Vec<Self::Item> {
        self.users
    }
}

///Represents a single-page view into a list of IDs.
///
///This type is intended to be used in the background by [`CursorIter`][] to hold an intermediate
///list of IDs to iterate over. See that struct's documentation for details.
///
///[`CursorIter`]: struct.CursorIter.html
#[derive(Deserialize)]
pub struct IDCursor {
    ///Numeric reference to the previous page of results.
    pub previous_cursor: i64,
    ///Numeric reference to the next page of results.
    pub next_cursor: i64,
    ///The list of user IDs in this page of results.
    pub ids: Vec<u64>,
}

impl Cursor for IDCursor {
    type Item = u64;
    type Id = NonZeroI64;

    fn previous_cursor_id(&self) -> Option<NonZeroI64> {
        NonZeroI64::new(self.previous_cursor)
    }

    fn next_cursor_id(&self) -> Option<NonZeroI64> {
        NonZeroI64::new(self.next_cursor)
    }

    fn into_inner(self) -> Vec<Self::Item> {
        self.ids
    }
}

///Represents a single-page view into a list of lists.
///
///This type is intended to be used in the background by [`CursorIter`][] to hold an intermediate
///list of lists to iterate over. See that struct's documentation for details.
///
///[`CursorIter`]: struct.CursorIter.html
#[derive(Deserialize)]
pub struct ListCursor {
    ///Numeric reference to the previous page of results.
    pub previous_cursor: i64,
    ///Numeric reference to the next page of results.
    pub next_cursor: i64,
    ///The list of lists in this page of results.
    pub lists: Vec<list::List>,
}

impl Cursor for ListCursor {
    type Item = list::List;
    type Id = NonZeroI64;

    fn previous_cursor_id(&self) -> Option<NonZeroI64> {
        NonZeroI64::new(self.previous_cursor)
    }

    fn next_cursor_id(&self) -> Option<NonZeroI64> {
        NonZeroI64::new(self.next_cursor)
    }

    fn into_inner(self) -> Vec<Self::Item> {
        self.lists
    }
}

/// Represents a paginated list of results, such as the users who follow a specific user or the
/// lists owned by that user.
///
/// This struct is given by several methods in this library, whenever Twitter would return a
/// cursored list of items. It implements the `Stream` trait, loading items in batches so that
/// several can be immedately returned whenever a single network call completes.
///
/// ```rust,no_run
/// # use egg_mode::Token;
/// use futures::{StreamExt, TryStreamExt};
///
/// # #[tokio::main]
/// # async fn main() {
/// # let token: Token = unimplemented!();
/// egg_mode::user::followers_of("rustlang", &token).take(10).try_for_each(|resp| {
///     println!("{}", resp.screen_name);
///     futures::future::ok(())
/// }).await.unwrap();
/// # }
/// ```
///
/// You can even collect the results, letting you get one set of rate-limit information for the
/// entire search setup:
///
/// ```rust,no_run
/// # use egg_mode::Token;
/// # #[tokio::main]
/// # async fn main() {
/// # let token: Token = unimplemented!();
/// use futures::{StreamExt, TryStreamExt};
/// use egg_mode::Response;
/// use egg_mode::user::TwitterUser;
/// use egg_mode::error::Result;
///
/// // Because Streams don't have a FromIterator adaptor, we load all the responses first, then
/// // collect them into the final Vec
/// let names: Result<Vec<TwitterUser>> =
///     egg_mode::user::followers_of("rustlang", &token)
///         .take(10)
///         .map_ok(|r| r.response)
///         .try_collect::<Vec<_>>()
///         .await;
/// # }
/// ```
///
/// `CursorIter` has an adaptor of its own, `with_page_size`, that you can use before consuming it.
/// `with_page_size` will let you set how many users are pulled in with a single network call.
/// Calling it after starting iteration will clear any current results.
///
/// (A note about `with_page_size`/`page_size`: While the `CursorIter` struct always has this method
/// and field available, not every cursored call supports changing page size. Check the individual
/// method documentation for notes on what page sizes are allowed.)
///
/// The `Stream` implementation yields `Response<T::Item>` on a successful iteration, and `Error`
/// for errors, so network errors, rate-limit errors and other issues are passed directly through
/// in `poll()`. The `Stream` implementation will allow you to poll again after an error to
/// re-initiate the late network call; this way, you can wait for your network connection to return
/// or for your rate limit to refresh and try again with the same state.
///
/// ## Manual paging
///
/// The `Stream` implementation works by loading in a page of results (with size set by the
/// method's default or by `with_page_size`/the `page_size` field) when it's polled, and serving
/// the individual elements from that locally-cached page until it runs out. This can be nice, but
/// it also means that your only warning that something involves a network call is that the stream
/// returns `Poll::Pending`, by which time the network call has already started. If you want
/// to know that ahead of time, that's where the `call()` method comes in. By using `call()`, you
/// can get the cursor struct directly from Twitter. With that you can iterate over the results and
/// page forward and backward as needed:
///
/// ```rust,no_run
/// # use egg_mode::{Token, cursor::Cursor};
/// # #[tokio::main]
/// # async fn main() {
/// # let token: Token = unimplemented!();
/// let mut list = egg_mode::user::followers_of("rustlang", &token).with_page_size(20);
/// let resp = list.call().await.unwrap();
///
/// for user in resp.response.users {
///     println!("{} (@{})", user.name, user.screen_name);
/// }
///
/// list.next_cursor = resp.response.next_cursor_id();
/// let resp = list.call().await.unwrap();
///
/// for user in resp.response.users {
///     println!("{} (@{})", user.name, user.screen_name);
/// }
/// # }
/// ```
#[must_use = "cursor iterators are lazy and do nothing unless consumed"]
pub struct CursorIter<T>
where
    T: Cursor + DeserializeOwned,
{
    link: &'static str,
    token: auth::Token,
    params_base: Option<ParamList>,
    ///The number of results returned in one network call.
    ///
    ///Certain calls set their own minimums and maximums for what this value can be. Furthermore,
    ///some calls don't allow you to set the size of the pages at all. Refer to the individual
    ///methods' documentation for specifics.
    pub page_size: Option<i32>,
    ///Id for the previous page of results. A value of [`None`] indicates that the current page of
    ///results is the first page of the cursor or that the given [`Cursor`] implementation does not
    ///return previous cursor ids.
    ///
    ///This value is intended to be automatically set and used as part of this struct's Iterator
    ///implementation. It is made available for those who wish to manually manage network calls and
    ///pagination.
    pub previous_cursor: Option<T::Id>,
    ///Numeric reference to the next page of results. A value of [`None`] indicates that the current
    ///page of results is the last page of the cursor.
    ///
    ///This value is intended to be automatically set and used as part of this struct's Iterator
    ///implementation. It is made available for those who wish to manually manage network calls and
    ///pagination.
    pub next_cursor: Option<T::Id>,
    loader: Option<FutureResponse<T>>,
    iter: Option<Box<dyn Iterator<Item = Response<T::Item>> + Send>>,
}

impl<T> CursorIter<T>
where
    T: Cursor + DeserializeOwned,
{
    ///Sets the number of results returned in a single network call.
    ///
    ///Certain calls set their own minimums and maximums for what this value can be. Furthermore,
    ///some calls don't allow you to set the size of the pages at all. Refer to the individual
    ///methods' documentation for specifics. If this method is called for a response that does not
    ///accept changing the page size, no change to the underlying struct will occur.
    ///
    ///Calling this function will invalidate any current results, if any were previously loaded.
    pub fn with_page_size(self, page_size: i32) -> CursorIter<T> {
        if self.page_size.is_some() {
            CursorIter {
                page_size: Some(page_size),
                previous_cursor: None,
                next_cursor: None,
                loader: None,
                iter: None,
                ..self
            }
        } else {
            self
        }
    }

    ///Loads the next page of results.
    ///
    ///This is intended to be used as part of this struct's Iterator implementation. It is provided
    ///as a convenience for those who wish to manage network calls and pagination manually.
    pub fn call(&self) -> impl Future<Output = Result<Response<T>>> {
        let params = self.params_base.as_ref().cloned().unwrap_or_default()
            .add_opt_param(T::STARTING_CURSOR_PARAMETER_NAME, self.next_cursor.map_string())
            .add_opt_param(T::COUNT_PARAMETER_NAME, self.page_size.map_string());

        let req = get(self.link, &self.token, Some(&params));
        request_with_json_response(req)
    }

    ///Creates a new instance of CursorIter, with the given parameters and empty initial results.
    ///
    ///This is essentially an internal infrastructure function, not meant to be used from consumer
    ///code.
    pub(crate) fn new(
        link: &'static str,
        token: &auth::Token,
        params_base: Option<ParamList>,
        page_size: Option<i32>,
    ) -> CursorIter<T> {
        CursorIter {
            link,
            token: token.clone(),
            params_base,
            page_size,
            previous_cursor: None,
            next_cursor: None,
            loader: None,
            iter: None,
        }
    }
}

impl<T> Stream for CursorIter<T>
where
    T: Cursor + DeserializeOwned + 'static,
    T::Item: Unpin + Send,
{
    type Item = Result<Response<T::Item>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        if let Some(mut fut) = self.loader.take() {
            match Pin::new(&mut fut).poll(cx) {
                Poll::Pending => {
                    self.loader = Some(fut);
                    return Poll::Pending;
                }
                Poll::Ready(Ok(resp)) => {
                    self.previous_cursor = resp.previous_cursor_id();
                    self.next_cursor = resp.next_cursor_id();

                    let resp = Response::map(resp, |r| r.into_inner());
                    let rate = resp.rate_limit_status;

                    let mut iter = Box::new(resp.response.into_iter().map(move |item| Response {
                        rate_limit_status: rate,
                        response: item,
                    }));
                    let first = iter.next();
                    self.iter = Some(iter);

                    match first {
                        Some(item) => return Poll::Ready(Some(Ok(item))),
                        None => return Poll::Ready(None),
                    }
                }
                Poll::Ready(Err(e)) => return Poll::Ready(Some(Err(e))),
            }
        }

        if let Some(ref mut results) = self.iter {
            if let Some(item) = results.next() {
                return Poll::Ready(Some(Ok(item)));
            } else if self.next_cursor.is_none() {
                return Poll::Ready(None);
            }
        }

        self.loader = Some(Box::pin(self.call()));
        self.poll_next(cx)
    }
}
