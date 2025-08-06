use anyhow::Result;
use proto::api::function_runner_service_client::FunctionRunnerServiceClient;

pub async fn run(name: String) -> Result<()> {
    let mut client = FunctionRunnerServiceClient::connect("http://[::1]:54036").await?;

    let request = tonic::Request::new(proto::api::InvokeRequest {
        payload: Some("{\"name\":\"".to_string() + &name + "\"}")
    });

    let response = client.invoke(request).await?;

    println!("{:?}", response.into_inner().output);

    Ok(())
}
