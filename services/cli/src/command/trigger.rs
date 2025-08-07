use anyhow::Result;
use proto::api::action::{
    function_runner_service_client::FunctionRunnerServiceClient,
    InvokeRequest
};

pub async fn run(name: String, body: String) -> Result<()> {
    println!("Sending to {}", name);
    let mut client = FunctionRunnerServiceClient::connect("http://[::1]:54036").await?;

    let request = tonic::Request::new(InvokeRequest {
        payload: Some(body)
    });

    let response = client.invoke(request).await?;

    println!("{:?}", response.into_inner().output);

    Ok(())
}
