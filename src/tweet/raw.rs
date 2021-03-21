use crate::{place, user};
use chrono;
use serde::Deserialize;
use url::Url;

use crate::common::serde_datetime;

use super::{
    deserialize_tweet_source, ExtendedTweetEntities, FilterLevel, Tweet,
    TweetEntities, TweetSource,
};

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RawTweet {
    pub coordinates: Option<RawCoordinates>,
    #[serde(with = "serde_datetime")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub current_user_retweet: Option<CurrentUserRetweet>,
    pub display_text_range: Option<(usize, usize)>,
    pub entities: TweetEntities,
    pub extended_entities: Option<ExtendedTweetEntities>,
    pub extended_tweet: Option<RawExtendedTweet>,
    pub favorite_count: i32,
    pub favorited: Option<bool>,
    pub filter_level: Option<FilterLevel>,
    pub id: u64,
    pub in_reply_to_user_id: Option<u64>,
    pub in_reply_to_screen_name: Option<String>,
    pub in_reply_to_status_id: Option<u64>,
    pub lang: Option<String>,
    pub place: Option<place::Place>,
    pub possibly_sensitive: Option<bool>,
    pub quoted_status_id: Option<u64>,
    pub quoted_status: Option<Box<Tweet>>,
    pub retweet_count: i32,
    pub retweeted: Option<bool>,
    pub retweeted_status: Option<Box<Tweet>>,
    #[serde(deserialize_with = "deserialize_tweet_source")]
    pub source: Option<TweetSource>,
    pub text: Option<String>,
    pub full_text: Option<String>,
    pub truncated: bool,
    pub user: Option<Box<user::TwitterUser>>,
    #[serde(default)]
    pub withheld_copyright: bool,
    pub withheld_in_countries: Option<Vec<String>>,
    pub withheld_scope: Option<String>,
}

/// A type that can be used to map the fields returned from the Twitter V2 API into the (V1 based)
/// [`Tweet`](super::Tweet) type.
///
/// A full list of fields available on tweets when using the V2 API is available [here][docs].
///
/// [docs]: https://developer.twitter.com/en/docs/twitter-api/data-dictionary/object-model/tweet
#[derive(Debug, Clone, Deserialize)]
pub struct RawTweetV2 {
    // Always present.
    pub(crate) id: u64,
    // Always present.
    pub(crate) text: String,

    pub(crate) attachments: Option<v2_supporting_structs::Attachments>,
    pub(crate) author_id: Option<String>,
    pub(crate) context_annotations: Option<Vec<v2_supporting_structs::ContextAnnotation>>,
    pub(crate) conversation_id: Option<u64>,
    #[serde(with = "serde_datetime")]
    pub(crate) created_at: chrono::DateTime<chrono::Utc>, // TODO: this too should be optional.
    pub(crate) entities: Option<v2_supporting_structs::Entities>,
    pub(crate) geo: Option<v2_supporting_structs::Geo>,
    pub(crate) in_reply_to_user_id: Option<u64>,
    pub(crate) lang: Option<String>,
    pub(crate) non_public_metrics: Option<v2_supporting_structs::NonPublicMetrics>,
    pub(crate) organic_metrics: Option<v2_supporting_structs::Metrics>,
    pub(crate) possibly_sensitive: Option<bool>,
    pub(crate) promoted_metrics: Option<v2_supporting_structs::Metrics>,
    pub(crate) public_metrics: Option<v2_supporting_structs::PublicMetrics>,
    pub(crate) referenced_tweets: Option<Vec<v2_supporting_structs::ReferencedTweet>>,
    pub(crate) reply_settings: Option<v2_supporting_structs::ReplySettings>,
    pub(crate) source: Option<TweetSource>,
    pub(crate) withheld: Option<v2_supporting_structs::WitheldDetails>,
}

impl RawTweetV2 {
    /// The V2 API requires that you specify which fields you want the server to send back.
    ///
    /// This function returns the list of fields that need to be present in order to turn a
    /// [`RawTweetV2`] into a [`RawTweet`] and then a [`Tweet`](super::Tweet).
    pub const fn fields_needed_for_v1_raw_tweet() -> &'static str {
        "\
        created_at,\
        entities,\
        geo,\
        in_reply_to_user_id,\
        lang,\
        possibly_sensitive,\
        public_metrics,\
        source,\
        withheld"
    }
}
/// Everything in this module comes from [here].
///
/// [here]: https://developer.twitter.com/en/docs/twitter-api/data-dictionary/object-model/tweet
pub(crate) mod v2_supporting_structs {
    use super::{Deserialize, RawCoordinates, Url};

    #[derive(Debug, Clone, Deserialize)]
    pub enum Attachments {
        #[serde(rename = "poll_ids")]
        PollIds(Vec<String>),
        #[serde(rename = "media_keys")]
        MediaKeys(Vec<String>),
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct ContextAnnotation {
        pub(crate) domain: ContextAnnotationDomain,
        pub(crate) entity: Option<ContextAnnotationEntity>,
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct ContextAnnotationDomain {
        pub(crate) id: u64,
        pub(crate) name: String,
        pub(crate) description: String,
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct ContextAnnotationEntity {
        pub(crate) id: u64,
        pub(crate) name: String,
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct Entities {
        pub(crate) annotations: Vec<Annotation>,
        pub(crate) cashtags: Vec<Cashtag>,
        pub(crate) hashtags: Vec<Hashtag>,
        pub(crate) mentions: Vec<Mention>,
        pub(crate) urls: Vec<UrlEntity>,
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct Annotation {
        pub(crate) start: u8,
        pub(crate) end: u8,
        pub(crate) probability: f32,
        pub(crate) r#type: String,
        pub(crate) normalized_text: String,
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct Cashtag {
        pub(crate) start: u8,
        pub(crate) end: u8,
        pub(crate) tag: String,
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct Hashtag {
        pub(crate) start: u8,
        pub(crate) end: u8,
        pub(crate) tag: String,
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct Mention {
        pub(crate) start: u8,
        pub(crate) end: u8,
        pub(crate) tag: String,
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct UrlEntity {
        pub(crate) start: u8,
        pub(crate) end: u8,
        pub(crate) url: Url,
        pub(crate) expanded_url: Url,
        pub(crate) display_url: String,
        pub(crate) status: u16,
        pub(crate) titel: String,
        pub(crate) description: Option<String>,
        pub(crate) unwound_url: Url,
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct Geo {
        pub(crate) coordinates: RawCoordinates,
        pub(crate) place_id: String,
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct NonPublicMetrics {
        pub(crate) impression_count: usize,
        pub(crate) url_link_clicks: usize,
        pub(crate) user_profile_clicks: usize,
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct Metrics {
        pub(crate) impression_count: usize,
        pub(crate) like_count: usize,
        pub(crate) reply_count: usize,
        pub(crate) retweet_count: usize,
        pub(crate) url_link_clicks: usize,
        pub(crate) user_profile_clicks: usize,
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct PublicMetrics {
        pub(crate) retweet_count: usize,
        pub(crate) reply_count: usize,
        pub(crate) like_count: usize,
        pub(crate) quote_count: usize,
    }

    #[derive(Debug, Clone, Deserialize)]
    #[serde(tag = "type", rename_all = "snake_case")]
    pub enum ReferencedTweet {
        RepliedTo { id: u64 },
        Quoted { id: u64 },
    }

    #[derive(Debug, Clone, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum ReplySettings {
        Everyone,
        MentionedUsers,
        Followers,
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct WitheldDetails {
        pub(crate) copyright: bool,
        pub(crate) country_codes: Vec<String>,
    }
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RawExtendedTweet {
    pub full_text: String,
    pub display_text_range: Option<(usize, usize)>,
    pub entities: TweetEntities,
    pub extended_entities: Option<ExtendedTweetEntities>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RawCoordinates {
    #[serde(rename = "type")]
    pub kind: String,
    pub coordinates: (f64, f64),
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CurrentUserRetweet {
    pub id: u64,
}
