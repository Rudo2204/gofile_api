use std::{
    collections::{
        HashMap,
    },
    str::{
        FromStr,
    },
};

use url::{
    Url,
};

use uuid::{
    Uuid,
};

use chrono::{
    DateTime,
    Utc,
    serde::{
        ts_seconds,
    },
};

use mime::{
    Mime,
};

use serde::{
    Serialize,
    Serializer,
    Deserialize,
    Deserializer,
    de,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateFolderApiPayload {
    pub token: String,
    pub parent_folder_id: Uuid,
    pub folder_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateContentApiPayload {
    pub token: String,

    #[serde(flatten)]
    pub opt: ContentOpt,
}

#[derive(Debug, Serialize)]
#[serde(tag = "option", content = "value", rename_all = "camelCase")]
pub enum ContentOpt {
    #[serde(serialize_with = "to_string")]
    Public(bool),

    Password(String),
    Description(String),

    #[serde(with = "ts_seconds")]
    Expire(DateTime<Utc>),

    #[serde(serialize_with = "comma_separated_string_from_vec")]
    Tags(Vec<String>),

    #[serde(serialize_with = "to_string")]
    DirectLink(bool),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyContentApiPayload {
    pub token: String,

    #[serde(serialize_with = "comma_separated_string_from_vec")]
    pub contents_id: Vec<Uuid>,
    pub folder_id_dest: Uuid,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteContentApiPayload {
    pub token: String,

    #[serde(serialize_with = "comma_separated_string_from_vec")]
    pub contents_id: Vec<Uuid>,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResult<T> {
    pub status: String,
    pub data: T,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Servers {
    pub servers: Vec<Server>,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    pub name: String,
    pub zone: String,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadedFile {
    pub guest_token: Option<String>,
    pub download_page: Url,
    pub code: String,
    pub parent_folder: Uuid,
    pub file_id: Uuid,
    pub file_name: String,

    #[serde(with = "hex::serde")]
    pub md5: [u8; 16],
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Content {
    pub id: Uuid,
    pub name: String,
    pub parent_folder: Uuid,

    #[serde(with = "ts_seconds")]
    pub create_time: DateTime<Utc>,

    #[serde(flatten)]
    pub kind: ContentKind,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(tag="type", rename_all = "camelCase")]
pub enum ContentKind {
    #[serde(rename_all = "camelCase")]
    Folder {
       code: String,

       #[serde(default)]
       public: bool,

       children_ids: Vec<Uuid>,

        // only top folder
       total_download_count: Option<u32>,
       total_size: Option<u64>,
       contents: Option<HashMap<Uuid, Content>>,
    },

    #[serde(rename_all = "camelCase")]
    File {
       size: u64,
       download_count: u32,

        #[serde(with = "hex::serde")]
       md5: [u8; 16],

        #[serde(deserialize_with = "mime_from_str")]
       mimetype: Mime,
       server_choosen: String,
       link: Url,
    },
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountDetails {
    pub id: Uuid,
    pub token: String,
    pub email: String,
    pub tier: String,
    pub root_folder: Uuid,
    pub files_count: u32,
    pub total_size: u64,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoInfo {
}

fn mime_from_str<'de, D>(d: D) -> Result<Mime, D::Error>
where
    D: Deserializer<'de>
{
    let mime_str = String::deserialize(d)?;
    match Mime::from_str(&mime_str) {
        Err(err) => Err(de::Error::custom(format!("{}", err))),
        Ok(mime) => Ok(mime),
    }
}

fn comma_separated_string_from_vec<T, S>(vec: &Vec<T>, s: S) -> Result<S::Ok, S::Error>
where
    T: ToString,
    S: Serializer
{
    let comma_separated_str = vec.iter().map(|s| s.to_string()).collect::<Vec<String>>().join(",");
    s.serialize_str(&comma_separated_str)
}

fn to_string<T, S>(v: T, s: S) -> Result<S::Ok, S::Error>
where
    T: ToString,
    S: Serializer
{
    s.serialize_str(&v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::uuid;
    use serde::de::DeserializeOwned;
    use serde_json::*;
    use chrono::prelude::*;
    use std::fmt::Debug;

    #[test]
    fn serialize() {
        assert_serialize(
            json!({
                "token": "foo",
                "parentFolderId": "00000000-0000-0000-0000-000000000001",
                "folderName": "bar",
            }),
            CreateFolderApiPayload {
                token: String::from("foo"),
                parent_folder_id: uuid!("00000000-0000-0000-0000-000000000001"),
                folder_name: String::from("bar"),
            },
        );
        assert_serialize(
            json!({ "token": "foo", "option": "public", "value": "true" }),
            UpdateContentApiPayload { token: String::from("foo"), opt: ContentOpt::Public(true) },
        );
        assert_serialize(
            json!({ "token": "foo", "option": "public", "value": "false" }),
            UpdateContentApiPayload { token: String::from("foo"), opt: ContentOpt::Public(false) },
        );
        assert_serialize(
            json!({ "token": "foo", "option": "password", "value": "bar" }),
            UpdateContentApiPayload { token: String::from("foo"), opt: ContentOpt::Password(String::from("bar")) },
        );
        assert_serialize(
            json!({ "token": "foo", "option": "description", "value": "bar" }),
            UpdateContentApiPayload { token: String::from("foo"), opt: ContentOpt::Description(String::from("bar")) },
        );
        assert_serialize(
            json!({ "token": "foo", "option": "expire", "value": 1000000000 }),
            UpdateContentApiPayload { token: String::from("foo"), opt: ContentOpt::Expire(Utc.with_ymd_and_hms(2001, 9, 9, 1, 46, 40).unwrap()) },
        );
        assert_serialize(
            json!({ "token": "foo", "option": "tags", "value": "bar,baz" }),
            UpdateContentApiPayload { token: String::from("foo"), opt: ContentOpt::Tags(vec![String::from("bar"), String::from("baz")]) },
        );
        assert_serialize(
            json!({ "token": "foo", "option": "directLink", "value": "false" }),
            UpdateContentApiPayload { token: String::from("foo"), opt: ContentOpt::DirectLink(false) },
        );
        assert_serialize(
            json!({
                "token": "foo",
                "contentsId": "00000000-0000-0000-0000-000000000001,00000000-0000-0000-0000-000000000002",
                "folderIdDest": "00000000-0000-0000-0000-000000000003",
            }),
            CopyContentApiPayload {
                token: String::from("foo"),
                contents_id: vec![uuid!("00000000-0000-0000-0000-000000000001"), uuid!("00000000-0000-0000-0000-000000000002")],
                folder_id_dest: uuid!("00000000-0000-0000-0000-000000000003"),
            },
        );
        assert_serialize(
            json!({
                "token": "foo",
                "contentsId": "00000000-0000-0000-0000-000000000001,00000000-0000-0000-0000-000000000002",
            }),
            DeleteContentApiPayload {
                token: String::from("foo"),
                contents_id: vec![uuid!("00000000-0000-0000-0000-000000000001"), uuid!("00000000-0000-0000-0000-000000000002")],
            },
        );
    }

    fn assert_serialize<T>(expected_value: Value, payload: T)
        where
            T: Serialize + Debug,
    {
        assert!(0 < format!("{:?}", payload).len());
        assert_eq!(expected_value, to_value(&payload).unwrap());
    }

    #[test]
    fn deserialize() {
        assert_deserialize(json!({ "status": "ok", "data": {} }), ApiResult { status: String::from("ok"), data: NoInfo { } });
        assert_deserialize(
            json!({ "status": "ok", "data": { "servers": [{ "name": "foo", "zone": "ja" }] } }),
            ApiResult { status: String::from("ok"), data: Servers { servers: vec![Server { name: String::from("foo"), zone: String::from("ja") }] } },
        );
        assert_deserialize(
            json!({
                "downloadPage": "http://example.com/path/file.txt",
                "code": "bar",
                "parentFolder": "00000000-0000-0000-0000-000000000001",
                "fileId": "00000000-0000-0000-0000-000000000002",
                "fileName": "baz",
                "md5": "000000000000000000000000000001ff",
            }),
            UploadedFile {
                guest_token: None,
                download_page: Url::parse("http://example.com/path/file.txt").unwrap(),
                code: String::from("bar"),
                parent_folder: uuid!("00000000-0000-0000-0000-000000000001"),
                file_id: uuid!("00000000-0000-0000-0000-000000000002"),
                file_name: String::from("baz"),
                md5: [
                    0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0x1, 0xff,
                ],
            },
        );
        assert_deserialize(
            json!({
                "guestToken": "foo",
                "downloadPage": "http://example.com/path/file.txt",
                "code": "bar",
                "parentFolder": "00000000-0000-0000-0000-000000000001",
                "fileId": "00000000-0000-0000-0000-000000000002",
                "fileName": "baz",
                "md5": "000000000000000000000000000001ff",
            }),
            UploadedFile {
                guest_token: Some(String::from("foo")),
                download_page: Url::parse("http://example.com/path/file.txt").unwrap(),
                code: String::from("bar"),
                parent_folder: uuid!("00000000-0000-0000-0000-000000000001"),
                file_id: uuid!("00000000-0000-0000-0000-000000000002"),
                file_name: String::from("baz"),
                md5: [
                    0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0x1, 0xff,
                ],
            },
        );
        assert_deserialize(
            json!({
                "id": "00000000-0000-0000-0000-000000000001",
                "token": "foo",
                "email": "bar",
                "tier": "baz",
                "rootFolder": "00000000-0000-0000-0000-000000000002",
                "filesCount": 1, 
                "totalSize": 2,
            }),
            AccountDetails {
                id: uuid!("00000000-0000-0000-0000-000000000001"),
                token: String::from("foo"),
                email: String::from("bar"),
                tier: String::from("baz"),
                root_folder: uuid!("00000000-0000-0000-0000-000000000002"),
                files_count: 1,
                total_size: 2,
            },
        );
        assert_deserialize(
            json!({
                "id": "00000000-0000-0000-0000-000000000001",
                "name": "foo",
                "parentFolder": "00000000-0000-0000-0000-000000000002",
                "createTime": 1000000001,
                "type": "folder",
                "code": "bar",
                "childrenIds": [
                    "00000000-0000-0000-0000-000000000003",
                    "00000000-0000-0000-0000-000000000004",
                ],
                "totalDownloadCount": 10,
                "totalSize": 20,
                "contents": {
                    "00000000-0000-0000-0000-000000000003": {
                        "id": "00000000-0000-0000-0000-000000000003",
                        "name": "baz",
                        "parentFolder": "00000000-0000-0000-0000-000000000001",
                        "createTime": 1000000002,
                        "type": "folder",
                        "code": "fiz",
                        "public": true,
                        "childrenIds": [],
                    },
                    "00000000-0000-0000-0000-000000000004": {
                        "id": "00000000-0000-0000-0000-000000000004",
                        "name": "foz",
                        "parentFolder": "00000000-0000-0000-0000-000000000001",
                        "createTime": 1000000003,
                        "type": "file",
                        "size": 20,
                        "downloadCount": 10,
                        "md5": "000000000000000000000000000001ff",
                        "mimetype": "text/plain",
                        "serverChoosen": "fez",
                        "link": "http://example.com/path/file.txt",
                    },
                },
            }),
            Content {
                id: uuid!("00000000-0000-0000-0000-000000000001"),
                name: String::from("foo"),
                parent_folder: uuid!("00000000-0000-0000-0000-000000000002"),
                create_time: Utc.with_ymd_and_hms(2001, 9, 9, 1, 46, 41).unwrap(),
                kind: ContentKind::Folder {
                    code: String::from("bar"),
                    public: false,
                    children_ids: vec![
                        uuid!("00000000-0000-0000-0000-000000000003"),
                        uuid!("00000000-0000-0000-0000-000000000004"),
                    ],
                    total_download_count: Some(10),
                    total_size: Some(20),
                    contents: Some(HashMap::from_iter([
                        (
                            uuid!("00000000-0000-0000-0000-000000000003"),
                            Content {
                                id: uuid!("00000000-0000-0000-0000-000000000003"),
                                name: String::from("baz"),
                                parent_folder: uuid!("00000000-0000-0000-0000-000000000001"),
                                create_time: Utc.with_ymd_and_hms(2001, 9, 9, 1, 46, 42).unwrap(),
                                kind: ContentKind::Folder {
                                    code: String::from("fiz"),
                                    public: true,
                                    children_ids: vec![
                                    ],
                                    total_download_count: None,
                                    total_size: None,
                                    contents: None,
                                },
                            },
                        ),
                        (
                            uuid!("00000000-0000-0000-0000-000000000004"),
                            Content {
                                id: uuid!("00000000-0000-0000-0000-000000000004"),
                                name: String::from("foz"),
                                parent_folder: uuid!("00000000-0000-0000-0000-000000000001"),
                                create_time: Utc.with_ymd_and_hms(2001, 9, 9, 1, 46, 43).unwrap(),
                                kind: ContentKind::File {
                                    size: 20,
                                    download_count: 10,
                                    md5: [
                                        0, 0, 0, 0, 0, 0, 0, 0,
                                        0, 0, 0, 0, 0, 0, 0x1, 0xff,
                                    ],
                                    mimetype: Mime::from_str("text/plain").unwrap(),
                                    server_choosen: String::from("fez"),
                                    link: Url::parse("http://example.com/path/file.txt").unwrap(),
                                },
                            },
                        )
                    ])),
                },
            },
        );

        assert!(from_value::<Content>(json!({
            "id": "00000000-0000-0000-0000-000000000004",
            "name": "foz",
            "parentFolder": "00000000-0000-0000-0000-000000000001",
            "createTime": 1000000003,
            "type": "file",
            "size": 20,
            "downloadCount": 10,
            "md5": "000000000000000000000000000001ff",
            "mimetype": "error-mime-type",
            "serverChoosen": "fez",
            "link": "http://example.com/path/file.txt",
        })).is_err());

    }

    fn assert_deserialize<T>(expected_value: Value, payload: T)
        where
            T: DeserializeOwned + Debug + PartialEq,
    {
        assert!(0 < format!("{:?}", payload).len());
        assert_eq!(from_value::<T>(expected_value).unwrap(), payload);
    }

}


