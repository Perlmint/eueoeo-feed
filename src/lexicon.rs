use std::str::FromStr;

#[derive(Debug, serde_with::DeserializeFromStr, PartialEq, Eq)]
pub struct AtUri {
    pub authority: String,
    pub collection: Option<String>,
    pub rkey: Option<String>,
}

impl AtUri {
    pub fn new(authority: String, collection: Option<String>, rkey: Option<String>) -> Self {
        Self {
            authority,
            collection,
            rkey,
        }
    }

    pub fn with_auth(authority: String) -> Self {
        Self {
            authority,
            collection: None,
            rkey: None,
        }
    }

    pub fn with_auth_path(authority: String, path: String) -> Self {
        let mut splitted = path.split('/');
        let collection = splitted.next().map(ToString::to_string);
        let rkey = splitted.next().map(ToString::to_string);

        Self {
            authority,
            collection,
            rkey,
        }
    }
}

impl ToString for AtUri {
    fn to_string(&self) -> String {
        use std::fmt::Write;

        let mut ret = String::new();
        write!(ret, "at://{}", self.authority).unwrap();
        if let Some(collection) = &self.collection {
            write!(ret, "/{collection}").unwrap();
            if let Some(rkey) = &self.rkey {
                write!(ret, "/{rkey}").unwrap();
            }
        }

        ret
    }
}

#[derive(thiserror::Error, Debug)]
pub enum AtUriParseError {
    #[error("AtUri should start with at://")]
    InvalidProtocolPrefix,
}

impl FromStr for AtUri {
    type Err = AtUriParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if !s.starts_with("at://") {
            return Err(AtUriParseError::InvalidProtocolPrefix);
        }

        let s = &s[5..];
        let (authority, collection, rkey) = s
            .split_once('/')
            .map(|(d, r)| {
                let (c, r) = r
                    .split_once('/')
                    .map(|(c, r)| (Some(c), Some(r)))
                    .unwrap_or_else(|| (Some(s), None));
                (d, c, r)
            })
            .unwrap_or_else(|| (s, None, None));

        Ok(Self {
            authority: authority.to_string(),
            collection: collection.map(ToString::to_string),
            rkey: rkey.map(ToString::to_string),
        })
    }
}

pub mod app {
    pub mod bsky {
        pub mod feed {
            pub mod post {
                pub const ID: &str = "app.bsky.feed.post";

                #[derive(Debug, serde::Deserialize)]
                pub struct Record {
                    pub text: String,
                }
            }
            pub mod repost {
                use crate::lexicon::com::atproto::repo::StrongRef;

                pub const ID: &str = "app.bsky.feed.repost";

                #[derive(Debug, serde::Deserialize)]
                #[serde(rename_all = "camelCase")]
                pub struct Record {
                    pub subject: StrongRef,
                    pub created_at: String,
                }
            }
            pub mod like {
                use crate::lexicon::com::atproto::repo::StrongRef;

                pub const ID: &str = "app.bsky.feed.like";

                #[derive(Debug, serde::Deserialize)]
                #[serde(rename_all = "camelCase")]
                pub struct Record {
                    pub subject: StrongRef,
                    pub created_at: String,
                }
            }
            pub mod get_feed_skeleton {
                #[derive(Debug, serde::Deserialize)]
                pub struct QueryParams {
                    pub feed: String,
                    pub limit: u32,
                    pub cursor: Option<String>,
                }

                #[derive(Debug, serde::Serialize)]
                pub struct Feed {
                    pub post: String,
                }

                #[derive(Debug, serde::Serialize)]
                pub struct OutputSchema {
                    pub cursor: Option<String>,
                    pub feed: Vec<Feed>,
                }
            }
        }
        pub mod graph {
            pub mod follow {
                #[derive(Debug, serde::Deserialize)]
                #[serde(rename_all = "camelCase")]
                pub struct Record {
                    pub subject: String, // did
                    pub created_at: String,
                }
            }
        }
    }
}

pub mod com {
    pub mod atproto {
        pub mod repo {
            use std::str::FromStr;

            use crate::lexicon::AtUri;

            #[derive(Debug, PartialEq, Eq)]
            pub enum StrongRef {
                Valid { uri: AtUri, cid: String },
                Invalid,
            }

            impl<'de> serde::Deserialize<'de> for StrongRef {
                fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                where
                    D: serde::Deserializer<'de>,
                {
                    const FIELDS: &[&str] = &["uri", "cid"];

                    struct Visitor;

                    impl<'de> serde::de::Visitor<'de> for Visitor {
                        type Value = StrongRef;

                        fn expecting(
                            &self,
                            formatter: &mut std::fmt::Formatter,
                        ) -> std::fmt::Result {
                            formatter.write_str("struct with uri, cid fields")
                        }

                        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
                        where
                            A: serde::de::MapAccess<'de>,
                        {
                            let mut cid: Option<String> = None;
                            let mut uri: Option<String> = None;
                            while let Some(key) = map.next_key()? {
                                match key {
                                    "cid" => {
                                        if cid.is_some() {
                                            return Err(serde::de::Error::duplicate_field("cid"));
                                        }
                                        cid = Some(map.next_value()?)
                                    }
                                    "uri" => {
                                        if uri.is_some() {
                                            return Err(serde::de::Error::duplicate_field("uri"));
                                        }
                                        uri = Some(map.next_value()?)
                                    }
                                    "$type" => {
                                        let r#type: String = map.next_value()?;
                                        const TYPE: &str = "com.atproto.repo.strongRef";
                                        if r#type != TYPE {
                                            return Err(serde::de::Error::invalid_value(
                                                serde::de::Unexpected::Str(&r#type),
                                                &TYPE,
                                            ));
                                        }
                                    }
                                    _ => {
                                        return Err(serde::de::Error::unknown_field(key, FIELDS));
                                    }
                                }
                            }

                            let Some(cid) = cid else {
                                return Err(serde::de::Error::missing_field("cid"));
                            };
                            let Some(uri) = uri else {
                                return Err(serde::de::Error::missing_field("uri"));
                            };
                            let Ok(uri) = AtUri::from_str(&uri) else {
                                return Ok(StrongRef::Invalid);
                            };

                            return Ok(StrongRef::Valid { uri, cid });
                        }
                    }

                    deserializer.deserialize_struct("StrongRef", FIELDS, Visitor)
                }
            }

            #[test]
            fn test_parse_strong_ref() {
                assert_eq!(
                    serde_json::from_str::<StrongRef>(r#"{"cid":"","uri":"at://example.com"}"#)
                        .unwrap(),
                    StrongRef::Valid {
                        uri: AtUri {
                            authority: "example.com".to_string(),
                            collection: None,
                            rkey: None
                        },
                        cid: "".to_string(),
                    }
                );

                assert_eq!(
                    serde_json::from_str::<StrongRef>(r#"{"$type":"com.atproto.repo.strongRef","cid":"","uri":"at://example.com"}"#)
                        .unwrap(),
                    StrongRef::Valid {
                        uri: AtUri {
                            authority: "example.com".to_string(),
                            collection: None,
                            rkey: None
                        },
                        cid: "".to_string(),
                    }
                );

                assert_eq!(
                    serde_json::from_str::<StrongRef>(r#"{"cid":"","uri":""}"#).unwrap(),
                    StrongRef::Invalid
                );
            }
        }
        pub mod sync {
            pub mod subscribe_repos {
                use anyhow::Context;
                use rs_car::Cid;

                pub const ID: &str = "com.atproto.sync.subscribeRepos";

                #[derive(Debug, serde::Serialize)]
                pub struct QueryParams {
                    pub cursor: Option<u64>,
                }

                #[derive(Debug, serde::Deserialize)]
                pub struct CommonPart {
                    pub seq: u64,
                    pub time: String,
                }

                #[serde_with::serde_as]
                #[derive(Debug, serde::Deserialize)]
                #[serde(rename_all = "camelCase")]
                pub struct Commit {
                    #[serde(flatten)]
                    pub _common: CommonPart,
                    pub rebase: bool,
                    pub too_big: bool,
                    pub repo: String,
                    pub commit: Cid,
                    #[serde_as(as = "serde_with::DefaultOnError")]
                    pub prev: Option<Cid>,
                    pub rev: String,
                    pub since: Option<String>,
                    pub blocks: Option<serde_bytes::ByteBuf>,
                    pub ops: Vec<RepoOp>,
                    pub blobs: Vec<Cid>,
                }

                #[serde_with::serde_as]
                #[derive(Debug, serde::Deserialize)]
                #[serde(rename_all = "camelCase")]
                pub struct Identity {
                    #[serde(flatten)]
                    pub _common: CommonPart,
                    pub did: String,
                    pub handle: Option<String>,
                }

                #[serde_with::serde_as]
                #[derive(Debug, serde::Deserialize)]
                #[serde(rename_all = "camelCase")]
                pub struct Account {
                    #[serde(flatten)]
                    pub _common: CommonPart,
                    pub did: String,
                    pub active: bool,
                    pub status: Option<String>,
                }

                #[derive(Debug, serde::Deserialize)]
                pub struct Handle {
                    #[serde(flatten)]
                    pub _common: CommonPart,
                    pub did: String,
                    pub handle: String,
                }

                #[derive(Debug, serde::Deserialize)]
                #[serde(rename_all = "camelCase")]
                pub struct Migrate {
                    #[serde(flatten)]
                    pub _common: CommonPart,
                    pub did: String,
                    pub migrate_to: Option<String>,
                }

                #[derive(Debug, serde::Deserialize)]
                pub struct Tombstone {
                    #[serde(flatten)]
                    pub _common: CommonPart,
                    pub did: String,
                }

                #[derive(Debug, serde::Deserialize)]
                pub struct Info {
                    pub name: InfoName,
                    pub message: Option<String>,
                }

                #[derive(Debug, serde::Deserialize)]
                pub enum InfoName {
                    OutDatedCursor,
                }

                #[derive(Debug, serde::Deserialize)]
                pub struct RepoOp {
                    pub action: RepoOpAction,
                    pub path: String,
                    pub cid: Option<Cid>,
                }

                #[derive(Debug, serde::Deserialize)]
                #[serde(rename_all = "lowercase")]
                pub enum RepoOpAction {
                    Create,
                    Update,
                    Delete,
                }

                #[derive(Debug, serde::Deserialize)]
                #[serde(tag = "$type")]
                pub enum OutputSchema {
                    #[serde(rename = "com.atproto.sync.subscribeRepos#commit")]
                    Commit(Commit),
                    #[serde(rename = "com.atproto.sync.subscribeRepos#identity")]
                    Identity(Identity),
                    #[serde(rename = "com.atproto.sync.subscribeRepos#account")]
                    Account(Account),
                    #[serde(rename = "com.atproto.sync.subscribeRepos#handle")]
                    Handle(Handle),
                    #[serde(rename = "com.atproto.sync.subscribeRepos#migrate")]
                    Migrate(Migrate),
                    #[serde(rename = "com.atproto.sync.subscribeRepos#tombstone")]
                    Tombstone(Tombstone),
                    #[serde(rename = "com.atproto.sync.subscribeRepos#info")]
                    Info(Info),
                }

                impl OutputSchema {
                    pub fn from_cbor(tag: &str, bytes: &[u8]) -> anyhow::Result<Self> {
                        Ok(match tag {
                            "#commit" => OutputSchema::Commit(
                                serde_ipld_dagcbor::from_slice(bytes)
                                    .with_context(|| format!("tag: commit, data: {bytes:?}"))?,
                            ),
                            "#identity" => OutputSchema::Identity(
                                serde_ipld_dagcbor::from_slice(bytes)
                                    .with_context(|| format!("tag: identity, data: {bytes:?}"))?,
                            ),
                            "#account" => OutputSchema::Account(
                                serde_ipld_dagcbor::from_slice(bytes)
                                    .with_context(|| format!("tag: account, data: {bytes:?}"))?,
                            ),
                            "#handle" => OutputSchema::Handle(
                                serde_ipld_dagcbor::from_slice(bytes)
                                    .with_context(|| format!("tag: handle, data: {bytes:?}"))?,
                            ),
                            "#migrate" => OutputSchema::Migrate(
                                serde_ipld_dagcbor::from_slice(bytes)
                                    .with_context(|| format!("tag: migrate, data: {bytes:?}"))?,
                            ),
                            "#tombstone" => OutputSchema::Tombstone(
                                serde_ipld_dagcbor::from_slice(bytes)
                                    .with_context(|| format!("tag: tombstone, data: {bytes:?}"))?,
                            ),
                            "#info" => OutputSchema::Info(
                                serde_ipld_dagcbor::from_slice(bytes)
                                    .with_context(|| format!("tag: info, data: {bytes:?}"))?,
                            ),
                            unknown => return Err(anyhow::anyhow!("Unknown tag - {unknown}")),
                        })
                    }
                }
            }
        }
    }
}
