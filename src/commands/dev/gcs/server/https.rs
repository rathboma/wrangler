use super::preview_request;
use crate::commands::dev::gcs::headers::destructure_response;
use crate::commands::dev::server_config::ServerConfig;
use crate::commands::dev::tls;
use crate::commands::dev::utils::get_path_as_str;
use crate::terminal::{emoji, message};

use std::sync::{Arc, Mutex};

use chrono::prelude::*;
use futures_util::stream::StreamExt;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client as HyperClient, Request, Response, Server};
use hyper_rustls::HttpsConnector;
use tokio::net::TcpListener;

/// performs all logic that takes an incoming request
/// and routes it to the Workers runtime preview service
pub async fn https(
    server_config: ServerConfig,
    preview_id: Arc<Mutex<String>>,
) -> Result<(), failure::Error> {
    tls::generate_cert()?;

    // set up https client to connect to the preview service
    let https = HttpsConnector::new();
    let client = HyperClient::builder().build::<_, Body>(https);

    let listening_address = server_config.listening_address;

    // create a closure that hyper will use later to handle HTTP requests
    // this takes care of sending an incoming request along to
    // the uploaded Worker script and returning its response
    let service = make_service_fn(move |_| {
        let client = client.to_owned();
        let server_config = server_config.to_owned();
        let preview_id = preview_id.to_owned();
        async move {
            Ok::<_, failure::Error>(service_fn(move |req| {
                let client = client.to_owned();
                let server_config = server_config.to_owned();
                let preview_id = preview_id.lock().unwrap().to_owned();
                let version = req.version();

                // record the time of the request
                let now: DateTime<Local> = Local::now();

                // split the request into parts so we can read
                // what it contains and display in logs
                let (parts, body) = req.into_parts();

                let req_method = parts.method.to_string();

                // parse the path so we can send it to the preview service
                // we don't want to send "localhost:8787/path", just "/path"
                let path = get_path_as_str(&parts.uri);

                async move {
                    // send the request to the preview service
                    let resp = preview_request(
                        Request::from_parts(parts, body),
                        client,
                        preview_id.to_owned(),
                    )
                    .await?;
                    let (mut parts, body) = resp.into_parts();

                    // format the response for the user
                    destructure_response(&mut parts)?;
                    let resp = Response::from_parts(parts, body);

                    // print information about the response
                    // [2020-04-20 15:25:54] GET example.com/ HTTP/1.1 200 OK
                    println!(
                        "[{}] {} {}{} {:?} {}",
                        now.format("%Y-%m-%d %H:%M:%S"),
                        req_method,
                        server_config.host,
                        path,
                        version,
                        resp.status()
                    );
                    Ok::<_, failure::Error>(resp)
                }
            }))
        }
    });

    // Create a TCP listener via tokio.
    let mut tcp = TcpListener::bind(&listening_address).await?;
    let tls_acceptor = &tls::get_tls_acceptor()?;
    let incoming_tls_stream = tcp
        .incoming()
        .filter_map(move |s| async move {
            let client = match s {
                Ok(x) => x,
                Err(e) => {
                    eprintln!("Failed to accept client {}", e);
                    return None;
                }
            };
            match tls_acceptor.accept(client).await {
                Ok(x) => Some(Ok(x)),
                Err(e) => {
                    eprintln!("Client connection error {}", e);
                    message::info("Make sure to use https and `--insecure` with curl");
                    None
                }
            }
        })
        .boxed();

    let server = Server::builder(tls::HyperAcceptor {
        acceptor: incoming_tls_stream,
    })
    .serve(service);
    println!(
        "{} Listening on https://{}",
        emoji::EAR,
        listening_address.to_string()
    );

    message::info("Generated certificate is not verified, browsers will give a warning and curl will require `--insecure`");

    if let Err(e) = server.await {
        eprintln!("{}", e);
    }

    Ok(())
}
