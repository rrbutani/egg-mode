// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::ops::{Deref, DerefMut};

use crate::common::*;
use crate::error::{Error::InvalidResponse, Result};
use crate::user::UserID;
use crate::{auth, cursor, links};

use super::*;

///Lookup a single tweet by numeric ID.
pub async fn show(id: u64, token: &auth::Token) -> Result<Response<Tweet>> {
    let params = ParamList::new()
        .extended_tweets()
        .add_param("id", id.to_string())
        .add_param("include_my_retweet", "true")
        .add_param("include_ext_alt_text", "true");
    let req = get(links::statuses::SHOW, token, Some(&params));
    request_with_json_response(req).await
}

///Lookup the most recent 100 (or fewer) retweets of the given tweet.
///
///Use the `count` parameter to indicate how many retweets you would like to retrieve. If `count`
///is 0 or greater than 100, it will be defaulted to 100 before making the call.
pub async fn retweets_of(id: u64, count: u32, token: &auth::Token) -> Result<Response<Vec<Tweet>>> {
    let params = ParamList::new().extended_tweets().add_param(
        "count",
        if count == 0 || count > 100 {
            100
        } else {
            count
        }
        .to_string(),
    );

    let url = format!("{}/{}.json", links::statuses::RETWEETS_OF_STEM, id);
    let req = get(&url, token, Some(&params));
    request_with_json_response(req).await
}

///Lookup the user IDs that have retweeted the given tweet.
///
///Note that while loading the list of retweeters is a cursored search, it does not allow you to
///set the page size. Calling `with_page_size` on the iterator returned by this function will not
///change the page size used by the network call. Setting `page_size` manually may result in an
///error from Twitter.
pub fn retweeters_of(id: u64, token: &auth::Token) -> cursor::CursorIter<cursor::IDCursor> {
    let params = ParamList::new().add_param("id", id.to_string());
    cursor::CursorIter::new(links::statuses::RETWEETERS_OF, token, Some(params), None)
}

///Lookup tweet information for the given list of tweet IDs.
///
///This function differs from `lookup_map` in how it handles protected or nonexistent tweets.
///`lookup` gives a Vec of just the tweets it could load, leaving out any that it couldn't find.
pub async fn lookup<I: IntoIterator<Item = u64>>(
    ids: I,
    token: &auth::Token,
) -> Result<Response<Vec<Tweet>>> {
    let id_param = ids.into_iter().fold(String::new(), |mut acc, x| {
        if !acc.is_empty() {
            acc.push(',');
        }
        acc.push_str(&x.to_string());
        acc
    });
    let params = ParamList::new()
        .extended_tweets()
        .add_param("id", id_param)
        .add_param("include_ext_alt_text", "true");

    let req = post(links::statuses::LOOKUP, token, Some(&params));
    request_with_json_response(req).await
}

///Lookup tweet information for the given list of tweet IDs, and return a map indicating which IDs
///couldn't be found.
///
///This function differs from `lookup` in how it handles protected or nonexistent tweets.
///`lookup_map` gives a map containing every ID in the input slice; tweets that don't exist or
///can't be read by the authenticated user store `None` in the map, whereas tweets that could be
///loaded store `Some` and the requested status.
pub async fn lookup_map<I: IntoIterator<Item = u64>>(
    ids: I,
    token: &auth::Token,
) -> Result<Response<HashMap<u64, Option<Tweet>>>> {
    let id_param = ids.into_iter().fold(String::new(), |mut acc, x| {
        if !acc.is_empty() {
            acc.push(',');
        }
        acc.push_str(&x.to_string());
        acc
    });
    let params = ParamList::new()
        .extended_tweets()
        .add_param("id", id_param)
        .add_param("map", "true")
        .add_param("include_ext_alt_text", "true");

    let req = post(links::statuses::LOOKUP, token, Some(&params));
    let parsed = request_with_json_response::<serde_json::Value>(req).await?;
    let mut map = HashMap::new();

    for (key, val) in parsed
        .response
        .get("id")
        .and_then(|v| v.as_object())
        .ok_or_else(|| {
            InvalidResponse(
                "unexpected response for lookup_map",
                Some(parsed.response.to_string()),
            )
        })?
    {
        let id = key.parse::<u64>().or(Err(InvalidResponse(
            "could not parse id as integer",
            Some(key.to_string()),
        )))?;
        if val.is_null() {
            map.insert(id, None);
        } else {
            let tweet = Tweet::deserialize(val)?;
            map.insert(id, Some(tweet));
        }
    }

    Ok(Response::map(parsed, |_| map))
}

///Make a `Timeline` struct for navigating the collection of tweets posted by the authenticated
///user and the users they follow.
///
///This method has a default page size of 20 tweets, with a maximum of 200.
///
///Twitter will only return the most recent 800 tweets by navigating this method.
pub fn home_timeline(token: &auth::Token) -> Timeline {
    Timeline::new(links::statuses::HOME_TIMELINE, None, token)
}

///Make a `Timeline` struct for navigating the collection of tweets that mention the authenticated
///user's screen name.
///
///This method has a default page size of 20 tweets, with a maximum of 200.
///
///Twitter will only return the most recent 800 tweets by navigating this method.
pub fn mentions_timeline(token: &auth::Token) -> Timeline {
    Timeline::new(links::statuses::MENTIONS_TIMELINE, None, token)
}

///Make a `Timeline` struct for navigating the collection of tweets posted by the given user,
///optionally including or excluding replies or retweets.
///
///Attempting to load the timeline of a protected account will only work if the account is the
///authenticated user's, or if the authenticated user is an approved follower of the account.
///
///This method has a default page size of 20 tweets, with a maximum of 200. Note that asking to
///leave out replies or retweets will generate pages that may have fewer tweets than your requested
///page size; Twitter will load the requested number of tweets before removing replies and/or
///retweets.
///
///Twitter will only load the most recent 3,200 tweets with this method.
pub fn user_timeline<T: Into<UserID>>(
    acct: T,
    with_replies: bool,
    with_rts: bool,
    token: &auth::Token,
) -> Timeline {
    let params = ParamList::new()
        .extended_tweets()
        .add_user_param(acct.into())
        .add_param("exclude_replies", (!with_replies).to_string())
        .add_param("include_rts", with_rts.to_string());

    Timeline::new(links::statuses::USER_TIMELINE, Some(params), token)
}

///Make a `Timeline` struct for navigating the collection of tweets posted by the authenticated
///user that have been retweeted by others.
///
///This method has a default page size of 20 tweets, with a maximum of 100.
pub fn retweets_of_me(token: &auth::Token) -> Timeline {
    Timeline::new(links::statuses::RETWEETS_OF_ME, None, token)
}

///Make a `Timeline` struct for navigating the collection of tweets liked by the given user.
///
///This method has a default page size of 20 tweets, with a maximum of 200.
pub fn liked_by<T: Into<UserID>>(acct: T, token: &auth::Token) -> Timeline {
    let params = ParamList::new()
        .extended_tweets()
        .add_user_param(acct.into());
    Timeline::new(links::statuses::LIKES_OF, Some(params), token)
}

///Retweet the given status as the authenticated user.
///
///On success, the future returned by this function yields the retweet, with the original status
///contained in `retweeted_status`.
pub async fn retweet(id: u64, token: &auth::Token) -> Result<Response<Tweet>> {
    let params = ParamList::new().extended_tweets();
    let url = format!("{}/{}.json", links::statuses::RETWEET_STEM, id);
    let req = post(&url, token, Some(&params));
    request_with_json_response(req).await
}

///Unretweet the given status as the authenticated user.
///
///The given ID may either be the original status, or the ID of the authenticated user's retweet of
///it.
///
///On success, the future returned by this function yields the original tweet.
pub async fn unretweet(id: u64, token: &auth::Token) -> Result<Response<Tweet>> {
    let params = ParamList::new().extended_tweets();
    let url = format!("{}/{}.json", links::statuses::UNRETWEET_STEM, id);
    let req = post(&url, token, Some(&params));
    request_with_json_response(req).await
}

///Like the given status as the authenticated user.
///
///On success, the future returned by this function yields the liked tweet.
pub async fn like(id: u64, token: &auth::Token) -> Result<Response<Tweet>> {
    let params = ParamList::new()
        .extended_tweets()
        .add_param("id", id.to_string());
    let req = post(links::statuses::LIKE, token, Some(&params));
    request_with_json_response(req).await
}

///Clears a like of the given status as the authenticated user.
///
///On success, the future returned by this function yields the given tweet.
pub async fn unlike(id: u64, token: &auth::Token) -> Result<Response<Tweet>> {
    let params = ParamList::new()
        .extended_tweets()
        .add_param("id", id.to_string());
    let req = post(links::statuses::UNLIKE, token, Some(&params));
    request_with_json_response(req).await
}

///Delete the given tweet. The authenticated user must be the user who posted the given tweet.
///
///On success, the future returned by this function yields the given tweet.
pub async fn delete(id: u64, token: &auth::Token) -> Result<Response<Tweet>> {
    let params = ParamList::new().extended_tweets();
    let url = format!("{}/{}.json", links::statuses::DELETE_STEM, id);
    let req = post(&url, token, Some(&params));
    request_with_json_response(req).await
}

/// Wrapper for [`Tweet`].
///
/// Exists to paper over differences in the V2 API.
#[derive(Debug, Deserialize)]
#[serde(try_from = "RawTweetV2")]
pub struct TweetWrapper(Tweet);

impl Deref for TweetWrapper {
    type Target = Tweet;
    fn deref(&self) -> &Tweet { &self.0 }
}

impl DerefMut for TweetWrapper {
    fn deref_mut(&mut self) -> &mut Tweet { &mut self.0 }
}

impl TryFrom<RawTweetV2> for TweetWrapper {
    type Error = error::Error;
    fn try_from(raw: RawTweetV2) -> Result<Self> {
        Ok(Self(raw.try_into()?))
    }
}

///All the children of a particular tweet (replies), recursively.
pub async fn all_children(
    root_tweet_id: u64,
    token: &auth::Token,
) -> cursor::CursorIter<cursor::SearchCursor<TweetWrapper>> {
    let params = ParamList::new()
        .add_param("query", format!("conversation_id:{}", root_tweet_id))
        .add_param("tweet.fields", RawTweetV2::fields_needed_for_v1_raw_tweet());

    cursor::CursorIter::new(links::v2::search::RECENT, token, Some(params), Some(100))
}

///All the children of a particular tweet (replies), recursively.
pub async fn all_children_raw(
    root_tweet_id: u64,
    token: &auth::Token,
) -> cursor::CursorIter<cursor::SearchCursor<RawTweetV2>> {
    let params = ParamList::new()
        .add_param("query", format!("conversation_id:{}", root_tweet_id))
        .add_param("tweet.fields", RawTweetV2::all_fields());

    cursor::CursorIter::new(links::v2::search::RECENT, token, Some(params), Some(100))
}
