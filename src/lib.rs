use std::{
    borrow::{
        Cow,
    },
    path::{
        Path,
        PathBuf,
    },
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

use reqwest::{
    Response,
    StatusCode,
    Body,
    Method,
    multipart::{
        Form,
        Part,
    },
};

use serde::{
    Serialize,
    Deserialize,
    Deserializer,
    de::{
        self,
        DeserializeOwned,
    },
};

use serde_json::{
    json,
    Value,
};

use tokio::{
    fs::{
        File,
    },
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

#[derive(Debug)]
pub enum Error {
    HttpRequestError(Option<Url>, String),
    HttpStatusCodeError(Url, StatusCode),
    ApiStatusError(Url, String),
    InvalidFilePath(PathBuf, String),
    CouldntOpenFile(PathBuf, String),
    InvalidContentUrl(Url, String),
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Self::HttpRequestError(err.url().map(|u| u.clone()), err.to_string())
    }
}

#[derive(Debug)]
pub struct Api {
}

impl Api {
    pub fn authorize(token: impl Into<String>) -> AuthorizedApi {
        AuthorizedApi { token: token.into() }
    }

    pub async fn get_server() -> Result<ServerApi, Error> {
        Self::get("getServer").await
    }

    fn code_from_content_url(url: &Url) -> Result<String, Error> {
        let Some(mut segs) = url.path_segments() else {
            return Err(Error::InvalidContentUrl(url.clone(), "The content url must have path segments like '/d/XXXX'.".into()));
        };
        match segs.next() {
            Some("d") => (),
            _ => return Err(Error::InvalidContentUrl(url.clone(), "The first path segment of content url must be 'd'.".into())),
        };
        let Some(code) = segs.next() else {
            return Err(Error::InvalidContentUrl(url.clone(), "The content url must have two path segments like '/d/XXXX'.".into()));
        };
        Ok(code.into())
    }

    fn url(path: impl AsRef<str>) -> Url {
        let path = path.as_ref();
        Url::parse(&(format!("https://api.gofile.io/{}", path))).unwrap()
    }

    async fn get<T>(path: impl AsRef<str>) -> Result<T, Error>
        where
            T: DeserializeOwned,
    {
        Self::get_with_params(path, vec![]).await
    }

    async fn get_with_params<T>(path: impl AsRef<str>, params: Vec<(&'static str, String)>) -> Result<T, Error>
        where
            T: DeserializeOwned,
    {
        let mut url = Self::url(path);
        for (key, value) in params {
            url.query_pairs_mut().append_pair(key, &value);
        };
        
        let res = reqwest::get(url).await?;
        Self::parse_res(res).await
    }

    async fn put_with_json<T, S>(path: impl AsRef<str>, data: S) -> Result<T, Error>
        where
            T: DeserializeOwned,
            S: Serialize,
    {
        Self::request_with_json(Method::PUT, path, data).await
    }

    async fn delete_with_json<T, S>(path: impl AsRef<str>, data: S) -> Result<T, Error>
        where
            T: DeserializeOwned,
            S: Serialize,
    {
        Self::request_with_json(Method::DELETE, path, data).await
    }

    async fn request_with_json<T, S>(method: Method, path: impl AsRef<str>, data: S) -> Result<T, Error>
        where
            T: DeserializeOwned,
            S: Serialize,
    {
        let url = Self::url(path);
        let client = reqwest::Client::new();
        let res = client
            .request(method, url)
            .json(&data)
            .send()
            .await?;
        Self::parse_res(res).await
    }

    async fn parse_res<T>(res: Response) -> Result<T, Error>
        where
            T: DeserializeOwned,
    {
        let status = res.status();
        let url = res.url().clone();
        if status != StatusCode::OK {
            return match res.json::<ApiResponse<Value>>().await {
                Ok(res_obj) => Err(Error::ApiStatusError(url, res_obj.status)),
                Err(_) => Err(Error::HttpStatusCodeError(url, status)),
            };
        };

        let res_obj = res.json::<ApiResponse<T>>().await?;
        if res_obj.status != "ok" {
            return Err(Error::ApiStatusError(url, res_obj.status));
        };

        Ok(res_obj.data)
    }
}

#[derive(Clone, Debug)]
pub struct AuthorizedApi {
    pub token: String,
}

impl AuthorizedApi {
    pub async fn get_server(&self) -> Result<AuthorizedServerApi, Error> {
        let ServerApi { server } = Api::get_server().await?;
        Ok(AuthorizedServerApi { server, token: self.token.clone() })
    }

    pub async fn get_content(&self, url: &Url) -> Result<Content, Error> {
        let code = Api::code_from_content_url(url)?;
        self.get_content_by_code(code).await
    }

    pub async fn get_content_by_id(&self, content_id: Uuid) -> Result<Content, Error> {
        self.get_content_impl(content_id.to_string()).await
    }

    pub async fn get_content_by_code(&self, code: impl Into<String>) -> Result<Content, Error> {
        self.get_content_impl(code).await
    }

    async fn get_content_impl(&self, id_or_code: impl Into<String>) -> Result<Content, Error> {
        Api::get_with_params("getContent", vec![("contentId", id_or_code.into()), ("token", self.token.clone())]).await
    }

    pub async fn get_account_details(&self) -> Result<AccountDetails, Error> {
        Api::get_with_params("getAccountDetails", vec![("token", self.token.clone())]).await
    }

    pub async fn create_folder(&self, parent_folder_id: Uuid, folder_name: impl Into<String>) -> Result<Content, Error> {
        Api::put_with_json("createFolder", json!({
            "token": self.token.clone(),
            "parentFolderId": parent_folder_id.to_string(),
            "folderName": folder_name.into(),
        })).await
    }

    pub async fn set_public_option(&self, content_id: Uuid, public: bool) -> Result<NoInfo, Error> {
        self.set_option(content_id, "public", if public { "true" } else { "false" }).await
    }

    pub async fn set_password_option(&self, content_id: Uuid, password: impl Into<String>) -> Result<NoInfo, Error> {
        self.set_option(content_id, "password", password).await
    }

    pub async fn set_description_option(&self, content_id: Uuid, description: impl Into<String>) -> Result<NoInfo, Error> {
        self.set_option(content_id, "description", description).await
    }

    pub async fn set_expire_option(&self, content_id: Uuid, expire: DateTime<Utc>) -> Result<NoInfo, Error> {
        self.set_option(content_id, "expire", expire.timestamp().to_string()).await
    }

    pub async fn set_tags_option<S>(&self, content_id: Uuid, tags: Vec<S>) -> Result<NoInfo, Error>
    where
        S: Into<String>,
    {
        let tags = tags.into_iter().map(|t| t.into()).collect::<Vec<_>>().join(",");
        self.set_option(content_id, "tags", tags).await
    }

    pub async fn get_direct_link(&self, content_id: Uuid) -> Result<Url, Error> {
        self.set_direct_link_option(content_id, true).await
    }

    pub async fn disable_direct_link(&self, content_id: Uuid) -> Result<NoInfo, Error> {
        self.set_direct_link_option(content_id, false).await
    }

    pub async fn set_direct_link_option<T>(&self, content_id: Uuid, direct_link: bool) -> Result<T, Error>
    where
        T: DeserializeOwned
    {
        self.set_option(content_id, "directLink", if direct_link { "true" } else { "false" }).await
    }

    pub async fn set_option<T>(&self, content_id: Uuid, option: impl Into<String>, value: impl Into<String>) -> Result<T, Error>
    where
        T: DeserializeOwned
    {
        Api::put_with_json("setOption", json!({
            "token": self.token.clone(),
            "contentId": content_id.to_string(),
            "option": option.into(),
            "value": value.into(),
        })).await
    }

    pub async fn copy_content(&self, content_ids: Vec<Uuid>, dest_folder_id: Uuid) -> Result<NoInfo, Error> {
        Api::put_with_json("copyContent", json!({
            "token": self.token.clone(),
            "contentsId": content_ids.into_iter().map(|id| id.to_string()).collect::<Vec<_>>().join(","),
            "folderIdDest": dest_folder_id.to_string(),
        })).await
    }

    pub async fn delete_content(&self, content_ids: Vec<Uuid>) -> Result<NoInfo, Error> {
        Api::delete_with_json("deleteContent", json!({
            "token": self.token.clone(),
            "contentsId": content_ids.into_iter().map(|id| id.to_string()).collect::<Vec<_>>().join(","),
        })).await
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ServerApi {
    pub server: String,
}

impl ServerApi {
    pub async fn upload_file(&self, path: impl AsRef<Path>) -> Result<UploadedFile, Error> {
        let (filename, file) = Self::open_file(path).await?;
        self.upload_file_with_filename(filename.into(), file).await
    }

    pub async fn upload_file_to_folder(&self, folder_id: Uuid, path: impl AsRef<Path>) -> Result<UploadedFile, Error> {
        let (filename, file) = Self::open_file(path).await?;
        self.upload_file_with_filename_to_folder(folder_id, filename.into(), file).await
    }

    pub async fn upload_file_with_filename(&self, filename: Cow<'static, str>, body: impl Into<Body>) -> Result<UploadedFile, Error> {
        Self::upload_file_impl(&self.server, filename, body, None, None).await
    }

    pub async fn upload_file_with_filename_to_folder(&self, folder_id: Uuid, filename: Cow<'static, str>, body: impl Into<Body>) -> Result<UploadedFile, Error> {
        Self::upload_file_impl(&self.server, filename, body, Some(folder_id), None).await
    }

    pub async fn open_file(path: impl AsRef<Path>) -> Result<(String, File), Error> {
        let path = path.as_ref();
        let Some(filename) = path.file_name() else {
            return Err(Error::InvalidFilePath(path.into(), "Couldn't get the filename.".into()));
        };
        let Some(filename) = filename.to_str() else {
            return Err(Error::InvalidFilePath(path.into(), "The filename couldn't convert to a utf-8 stirng.".into()));
        };

        let file = match File::open(path).await {
            Ok(file) => file,
            Err(err) => return Err(Error::CouldntOpenFile(path.into(), format!("{}", err))),
        };

        Ok((filename.into(), file))
    }

    async fn upload_file_impl(server: &str, filename: Cow<'static, str>, body: impl Into<Body>, folder_id: Option<Uuid>, token: Option<String>) -> Result<UploadedFile, Error> {
        let client = reqwest::Client::new();

        let part = Part::stream(body)
            .file_name(filename);
        let form = Form::new()
            .part("file", part);

        let form = if let Some(folder_id) = folder_id {
            form.text("folderId", folder_id.to_string())
        } else {
            form
        };

        let form = if let Some(token) = token {
            form.text("token", token)
        } else {
            form
        };

        let url = Url::parse(&(format!("https://{}.gofile.io/uploadFile", server))).unwrap();

        let res = client.post(url)
            .multipart(form)
            .send()
            .await?;

        Api::parse_res(res).await
    }
}

#[derive(Clone, Debug)]
pub struct AuthorizedServerApi {
    pub server: String,
    pub token: String,
}

impl AuthorizedServerApi {
    pub fn authorize(self, token: impl Into<String>) -> AuthorizedServerApi {
        AuthorizedServerApi { server: self.server, token: token.into() }
    }

    pub async fn upload_file(&self, path: impl AsRef<Path>) -> Result<UploadedFile, Error> {
        let (filename, file) = ServerApi::open_file(path).await?;
        self.upload_file_with_filename(filename.into(), file).await
    }

    pub async fn upload_file_to_folder(&self, folder_id: Uuid, path: impl AsRef<Path>) -> Result<UploadedFile, Error> {
        let (filename, file) = ServerApi::open_file(path).await?;
        self.upload_file_with_filename_to_folder(folder_id, filename.into(), file).await
    }

    pub async fn upload_file_with_filename(&self, filename: Cow<'static, str>, body: impl Into<Body>) -> Result<UploadedFile, Error> {
        ServerApi::upload_file_impl(&self.server, filename, body, None, Some(self.token.clone())).await
    }

    pub async fn upload_file_with_filename_to_folder(&self, folder_id: Uuid, filename: Cow<'static, str>, body: impl Into<Body>) -> Result<UploadedFile, Error> {
        ServerApi::upload_file_impl(&self.server, filename, body, Some(folder_id), Some(self.token.clone())).await
    }
}

#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    status: String,
    data: T,
}

#[derive(Debug, Deserialize)]
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

impl UploadedFile {
    pub fn guest_api(&self) -> Option<AuthorizedApi> {
        if let Some(token) = &self.guest_token {
            Some(AuthorizedApi { token: token.clone() })
        } else {
            None
        }
    }
}

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
#[serde(tag="type", rename_all = "camelCase")]
pub enum ContentKind {
    #[serde(rename_all = "camelCase")]
    Folder {
       code: String,

       #[serde(default)]
       public: bool,

       childs: Vec<Uuid>,

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

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
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

