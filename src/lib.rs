mod payload;
use chrono::{DateTime, Utc};
use reqwest::{
    multipart::{Form, Part},
    Body, Method, Response, StatusCode,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tokio::fs::File;
use url::Url;
use uuid::Uuid;

pub use payload::*;

#[derive(Debug)]
pub enum Error {
    HttpRequestError(Option<Url>, String),
    HttpStatusCodeError(Url, StatusCode),
    ApiStatusError(Url, String),
    EmptyServerList,
    InvalidFilePath(PathBuf, String),
    CouldntOpenFile(PathBuf, String),
    InvalidContentUrl(Url, String),
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Self::HttpRequestError(err.url().cloned(), err.to_string())
    }
}

#[derive(Debug)]
pub struct Api {
    pub base_url: String,
}

impl Default for Api {
    fn default() -> Self {
        Self {
            base_url: "https://api.gofile.io".into(),
        }
    }
}

impl Api {
    pub fn authorize(&self, token: impl Into<String>) -> AuthorizedApi {
        AuthorizedApi {
            base_url: self.base_url.clone(),
            token: token.into(),
        }
    }

    pub async fn get_server(&self) -> Result<ServerApi, Error> {
        let Servers { servers } = Api::get(&self.base_url, "servers").await?;
        let server = servers
            .into_iter()
            .next()
            .ok_or(Error::EmptyServerList)?
            .name;
        Ok(ServerApi {
            base_url: format!("https://{}.gofile.io", server),
        })
    }

    fn code_from_content_url(url: &Url) -> Result<String, Error> {
        let Some(mut segs) = url.path_segments() else {
            return Err(Error::InvalidContentUrl(
                url.clone(),
                "The content url must have path segments like '/d/XXXX'.".into(),
            ));
        };
        match segs.next() {
            Some("d") => (),
            _ => {
                return Err(Error::InvalidContentUrl(
                    url.clone(),
                    "The first path segment of content url must be 'd'.".into(),
                ))
            }
        };
        let Some(code) = segs.next() else {
            return Err(Error::InvalidContentUrl(
                url.clone(),
                "The content url must have two path segments like '/d/XXXX'.".into(),
            ));
        };
        Ok(code.into())
    }

    fn url(base_url: impl AsRef<str>, path: impl AsRef<str>) -> Url {
        let path = path.as_ref();
        Url::parse(&(format!("{}/{}", base_url.as_ref(), path))).unwrap()
    }

    async fn get<T>(base_url: impl AsRef<str>, path: impl AsRef<str>) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        Self::get_with_params(base_url, path, vec![]).await
    }

    async fn get_with_params<T>(
        base_url: impl AsRef<str>,
        path: impl AsRef<str>,
        params: Vec<(&'static str, String)>,
    ) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        let mut url = Self::url(base_url, path);
        for (key, value) in params {
            url.query_pairs_mut().append_pair(key, &value);
        }

        let res = reqwest::get(url).await?;
        Self::parse_res(res).await
    }

    async fn put_with_payload<T, P>(
        base_url: impl AsRef<str>,
        path: impl AsRef<str>,
        payload: P,
    ) -> Result<T, Error>
    where
        T: DeserializeOwned,
        P: Serialize,
    {
        Self::request_with_payload(Method::PUT, base_url, path, payload).await
    }

    async fn delete_with_payload<T, P>(
        base_url: impl AsRef<str>,
        path: impl AsRef<str>,
        payload: P,
    ) -> Result<T, Error>
    where
        T: DeserializeOwned,
        P: Serialize,
    {
        Self::request_with_payload(Method::DELETE, base_url, path, payload).await
    }

    async fn request_with_payload<T, P>(
        method: Method,
        base_url: impl AsRef<str>,
        path: impl AsRef<str>,
        payload: P,
    ) -> Result<T, Error>
    where
        T: DeserializeOwned,
        P: Serialize,
    {
        let url = Self::url(base_url, path);
        let client = reqwest::Client::new();
        let res = client.request(method, url).json(&payload).send().await?;
        Self::parse_res(res).await
    }

    async fn parse_res<T>(res: Response) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        let status = res.status();
        let url = res.url().clone();
        if status != StatusCode::OK {
            return match res.json::<ApiResult<Value>>().await {
                Ok(res_obj) => Err(Error::ApiStatusError(url, res_obj.status)),
                Err(_) => Err(Error::HttpStatusCodeError(url, status)),
            };
        };

        let res_obj = res.json::<ApiResult<T>>().await?;
        if res_obj.status != "ok" {
            return Err(Error::ApiStatusError(url, res_obj.status));
        };

        Ok(res_obj.data)
    }
}

#[derive(Clone, Debug)]
pub struct AuthorizedApi {
    pub base_url: String,
    pub token: String,
}

impl AuthorizedApi {
    pub async fn get_server(&self) -> Result<AuthorizedServerApi, Error> {
        let ServerApi { base_url } = Api::default().get_server().await?;
        Ok(AuthorizedServerApi {
            base_url,
            token: self.token.clone(),
        })
    }

    pub async fn get_content(&self, url: &Url) -> Result<Content, Error> {
        let code = Api::code_from_content_url(url)?;
        self.get_content_by_code(code).await
    }

    pub async fn get_content_by_id(&self, content_id: Uuid) -> Result<Content, Error> {
        self.get_content_impl(content_id.to_string()).await
    }

    pub async fn get_content_by_code(&self, code: impl AsRef<str>) -> Result<Content, Error> {
        self.get_content_impl(code).await
    }

    async fn get_content_impl(&self, id_or_code: impl AsRef<str>) -> Result<Content, Error> {
        Api::get_with_params(
            &self.base_url,
            format!("contents/{}", id_or_code.as_ref()),
            vec![("token", self.token.clone())],
        )
        .await
    }

    pub async fn get_account_id(&self) -> Result<Uuid, Error> {
        Api::get_with_params(
            &self.base_url,
            "accounts/getid",
            vec![("token", self.token.clone())],
        )
        .await
    }

    pub async fn get_account_details(&self, account_id: Uuid) -> Result<AccountDetails, Error> {
        Api::get_with_params(
            &self.base_url,
            format!("accounts/{}", account_id),
            vec![("token", self.token.clone())],
        )
        .await
    }

    pub async fn create_folder(
        &self,
        parent_folder_id: Uuid,
        folder_name: impl Into<String>,
    ) -> Result<Content, Error> {
        Api::put_with_payload(
            &self.base_url,
            "contents/createFolder",
            CreateFolderApiPayload {
                token: self.token.clone(),
                parent_folder_id,
                folder_name: folder_name.into(),
            },
        )
        .await
    }

    pub async fn set_public_option(&self, content_id: Uuid, public: bool) -> Result<NoInfo, Error> {
        self.set_option(content_id, ContentOpt::Public(public))
            .await
    }

    pub async fn set_password_option(
        &self,
        content_id: Uuid,
        password: impl Into<String>,
    ) -> Result<NoInfo, Error> {
        self.set_option(content_id, ContentOpt::Password(password.into()))
            .await
    }

    pub async fn set_description_option(
        &self,
        content_id: Uuid,
        description: impl Into<String>,
    ) -> Result<NoInfo, Error> {
        self.set_option(content_id, ContentOpt::Description(description.into()))
            .await
    }

    pub async fn set_expire_option(
        &self,
        content_id: Uuid,
        expire: DateTime<Utc>,
    ) -> Result<NoInfo, Error> {
        self.set_option(content_id, ContentOpt::Expire(expire))
            .await
    }

    pub async fn set_tags_option<S>(&self, content_id: Uuid, tags: Vec<S>) -> Result<NoInfo, Error>
    where
        S: Into<String>,
    {
        self.set_option(
            content_id,
            ContentOpt::Tags(tags.into_iter().map(|s| s.into()).collect()),
        )
        .await
    }

    pub async fn get_direct_link(&self, content_id: Uuid) -> Result<Url, Error> {
        self.set_direct_link_option(content_id, true).await
    }

    pub async fn disable_direct_link(&self, content_id: Uuid) -> Result<NoInfo, Error> {
        self.set_direct_link_option(content_id, false).await
    }

    pub async fn set_direct_link_option<T>(
        &self,
        content_id: Uuid,
        direct_link: bool,
    ) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        self.set_option(content_id, ContentOpt::DirectLink(direct_link))
            .await
    }

    pub async fn set_option<T>(&self, content_id: Uuid, opt: ContentOpt) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        Api::put_with_payload(
            &self.base_url,
            format!("contents/{}/update", content_id),
            UpdateContentApiPayload {
                token: self.token.clone(),
                opt,
            },
        )
        .await
    }

    pub async fn copy_content(
        &self,
        content_ids: Vec<Uuid>,
        dest_folder_id: Uuid,
    ) -> Result<NoInfo, Error> {
        Api::put_with_payload(
            &self.base_url,
            "contents/copy",
            CopyContentApiPayload {
                token: self.token.clone(),
                contents_id: content_ids,
                folder_id_dest: dest_folder_id,
            },
        )
        .await
    }

    pub async fn delete_content(&self, content_ids: Vec<Uuid>) -> Result<NoInfo, Error> {
        Api::delete_with_payload(
            &self.base_url,
            "contents",
            DeleteContentApiPayload {
                token: self.token.clone(),
                contents_id: content_ids,
            },
        )
        .await
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ServerApi {
    pub base_url: String,
}

impl ServerApi {
    pub async fn upload_file(&self, path: impl AsRef<Path>) -> Result<UploadedFile, Error> {
        let (filename, file) = Self::open_file(path).await?;
        self.upload_file_with_filename(filename, file).await
    }

    pub async fn upload_file_to_folder(
        &self,
        folder_id: Uuid,
        path: impl AsRef<Path>,
    ) -> Result<UploadedFile, Error> {
        let (filename, file) = Self::open_file(path).await?;
        self.upload_file_with_filename_to_folder(folder_id, filename, file)
            .await
    }

    pub async fn upload_file_with_filename(
        &self,
        filename: impl Into<String>,
        body: impl Into<Body>,
    ) -> Result<UploadedFile, Error> {
        Self::upload_file_impl(&self.base_url, filename, body, None, None).await
    }

    pub async fn upload_file_with_filename_to_folder(
        &self,
        folder_id: Uuid,
        filename: impl Into<String>,
        body: impl Into<Body>,
    ) -> Result<UploadedFile, Error> {
        Self::upload_file_impl(&self.base_url, filename, body, Some(folder_id), None).await
    }

    pub async fn open_file(path: impl AsRef<Path>) -> Result<(String, File), Error> {
        let path = path.as_ref();
        let Some(filename) = path.file_name() else {
            return Err(Error::InvalidFilePath(
                path.into(),
                "Couldn't get the filename.".into(),
            ));
        };
        let Some(filename) = filename.to_str() else {
            return Err(Error::InvalidFilePath(
                path.into(),
                "The filename couldn't convert to a utf-8 stirng.".into(),
            ));
        };

        let file = match File::open(path).await {
            Ok(file) => file,
            Err(err) => return Err(Error::CouldntOpenFile(path.into(), format!("{}", err))),
        };

        Ok((filename.into(), file))
    }

    async fn upload_file_impl(
        base_url: &str,
        filename: impl Into<String>,
        body: impl Into<Body>,
        folder_id: Option<Uuid>,
        token: Option<String>,
    ) -> Result<UploadedFile, Error> {
        let client = reqwest::Client::new();

        let part = Part::stream(body).file_name(filename.into());
        let form = Form::new().part("file", part);

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

        let url = Url::parse(&(format!("{}/contents/uploadfile", base_url))).unwrap();

        let res = client.post(url).multipart(form).send().await?;

        Api::parse_res(res).await
    }
}

#[derive(Clone, Debug)]
pub struct AuthorizedServerApi {
    pub base_url: String,
    pub token: String,
}

impl AuthorizedServerApi {
    pub fn authorize(self, token: impl Into<String>) -> AuthorizedServerApi {
        AuthorizedServerApi {
            base_url: self.base_url,
            token: token.into(),
        }
    }

    pub async fn upload_file(&self, path: impl AsRef<Path>) -> Result<UploadedFile, Error> {
        let (filename, file) = ServerApi::open_file(path).await?;
        self.upload_file_with_filename(filename, file).await
    }

    pub async fn upload_file_to_folder(
        &self,
        folder_id: Uuid,
        path: impl AsRef<Path>,
    ) -> Result<UploadedFile, Error> {
        let (filename, file) = ServerApi::open_file(path).await?;
        self.upload_file_with_filename_to_folder(folder_id, filename, file)
            .await
    }

    pub async fn upload_file_with_filename(
        &self,
        filename: impl Into<String>,
        body: impl Into<Body>,
    ) -> Result<UploadedFile, Error> {
        ServerApi::upload_file_impl(
            &self.base_url,
            filename,
            body,
            None,
            Some(self.token.clone()),
        )
        .await
    }

    pub async fn upload_file_with_filename_to_folder(
        &self,
        folder_id: Uuid,
        filename: impl Into<String>,
        body: impl Into<Body>,
    ) -> Result<UploadedFile, Error> {
        ServerApi::upload_file_impl(
            &self.base_url,
            filename,
            body,
            Some(folder_id),
            Some(self.token.clone()),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::{Matcher, Server};
    use uuid::uuid;

    #[tokio::test]
    async fn it_works() -> Result<(), Error> {
        let mut server = Server::new();
        let base_url = server.url();

        assert_eq!(Api::default().base_url, "https://api.gofile.io");

        let api = Api {
            base_url: base_url.clone(),
        };
        let authorized_api = api.authorize("gofile_token");

        let mock = server.mock("GET", "/servers")
            .with_status(200)
            .with_body(r#"{ "status": "ok", "data": { "servers": [ {"name":"store1","zone":"eu"}, {"name":"store3","zone":"na"} ] } }"#)
            .expect(1)
            .create();
        let server_api = api.get_server().await?;
        assert_eq!(server_api.base_url, "https://store1.gofile.io");
        mock.assert();

        let mock = server
            .mock("POST", "/contents/uploadfile")
            .match_body(Matcher::Regex(String::from(r#"file content"#)))
            .with_status(200)
            .with_body(
                r#"{
                "status": "ok",
                "data": {
                    "guestToken": "foo",
                    "downloadPage": "http://example.com/path/file.txt",
                    "code": "bar",
                    "parentFolder": "00000000-0000-0000-0000-000000000001",
                    "fileId": "00000000-0000-0000-0000-000000000002",
                    "fileName": "baz",
                    "md5": "000000000000000000000000000001ff"
                }
            }"#,
            )
            .expect(1)
            .create();
        let server_api = ServerApi {
            base_url: base_url.clone(),
        };
        let uploaded_file = server_api
            .upload_file_with_filename("test.txt", "file content")
            .await?;
        assert_eq!(
            uploaded_file.file_id,
            uuid!("00000000-0000-0000-0000-000000000002")
        );
        mock.assert();

        let mock = server.mock("GET", "/contents/foo?token=gofile_token")
            .with_status(200)
            .with_body(r#"{
              "status": "ok",
              "data": {
                "id": "5e042945-0e5c-4c1d-9293-4574d376e496",
                "type": "folder",
                "name": "test3",
                "parentFolder": "9e67ed91-a838-48f6-a96f-f2dd13de3e31",
                "code": "JoKslp",
                "createTime": 1709956384,
                "public": true,
                "totalDownloadCount": 1,
                "totalSize": 120041,
                "childrenIds": [
                  "caf9aa4f-09fc-4e73-adb7-174a9c681674",
                  "cb35b4aa-274d-4d0f-9e81-97a58dd7fb37",
                  "191c0a18-e4b2-494b-92dc-cff7e7fc6471"
                ],
                "children": {
                  "191c0a18-e4b2-494b-92dc-cff7e7fc6471": {
                    "id": "191c0a18-e4b2-494b-92dc-cff7e7fc6471",
                    "type": "folder",
                    "name": "folder",
                    "code": "p8NOPG",
                    "createTime": 1710264457,
                    "public": true,
                    "childrenIds": [
                      "f8b5c54d-75b3-4593-beed-52c1379bf2ab"
                    ]
                  },
                  "cb35b4aa-274d-4d0f-9e81-97a58dd7fb37": {
                    "id": "cb35b4aa-274d-4d0f-9e81-97a58dd7fb37",
                    "type": "file",
                    "name": "public.zip",
                    "createTime": 1710264451,
                    "size": 26178,
                    "downloadCount": 0,
                    "md5": "c7dfde837b22280147a8cc2d9cb4d8a4",
                    "mimetype": "application/zip",
                    "serverSelected": "store2",
                    "link": "https://store2.gofile.io/download/web/cb35b4aa-274d-4d0f-9e81-97a58dd7fb37/public.zip"
                  },
                  "caf9aa4f-09fc-4e73-adb7-174a9c681674": {
                    "id": "caf9aa4f-09fc-4e73-adb7-174a9c681674",
                    "type": "file",
                    "name": "Capture.JPG",
                    "createTime": 1710264443,
                    "size": 93863,
                    "downloadCount": 1,
                    "md5": "b32e8427d3c8b25253a7559170b281d2",
                    "mimetype": "image/jpeg",
                    "serverSelected": "store5",
                    "link": "https://store5.gofile.io/download/web/caf9aa4f-09fc-4e73-adb7-174a9c681674/Capture.JPG",
                    "thumbnail": "https://store5.gofile.io/download/web/caf9aa4f-09fc-4e73-adb7-174a9c681674/thumb_Capture.JPG"
                  }
                }
              }
            }"#)
            .expect(1)
            .create();
        let content = authorized_api
            .get_content(&Url::parse("https://gofile.io/d/foo").unwrap())
            .await?;
        assert_eq!(content.id, uuid!("5e042945-0e5c-4c1d-9293-4574d376e496"));
        mock.assert();

        Ok(())
    }
}
