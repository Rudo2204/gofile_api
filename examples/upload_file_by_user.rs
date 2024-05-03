use gofile_api::*;
use std::env::*;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let token = var("GOFILE_TOKEN").unwrap();
    let file_path = &args().collect::<Vec<_>>()[1];

    let api = Api::new().authorize(token);
    let server_api = api.get_server().await?;
    let uploaded_file_info = server_api.upload_file(file_path).await?;
    println!("{:?}", uploaded_file_info);

    let folder_id = uploaded_file_info.parent_folder;
    api.set_public_option(folder_id, false).await?;

    Ok(())
}
