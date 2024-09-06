//! This is the rendering server for Verfassungsbooks instances.
//!
//! It listens to incoming TCP requests from a main server.
//! Only connections with a valid certificate signed by the CA are accepted (mTLS).
//!
//! # Deployment
//! ## Setup Certificates
//! You'll need an CA to sign the certificates for each server.
//! Warning: every server which can provide an certificate signed by the CA will be able to use the rendering server.
//!
//! ### Create a new CA
//! 0. You will need to have openssl installed. Create a new empty folder an cd to it
//! 1. Create a new config file ca.conf:
//! ```
//! [ca]
//! 
//! default_ca = default
//! 
//! [default]
//! 
//! dir = .
//! certs = $dir
//! new_certs_dir = $dir/db.certs
//! 
//! database = $dir/db.index
//! serial = $dir/db.serial
//! 
//! certificate = $dir/root.crt
//! private_key = $dir/root.key
//! 
//! default_days = 365
//! default_crl_days = 30
//!
//! default_md = sha256
//! 
//! preserve = no
//! policy = default_policy
//! 
//! [default_policy]
//! 
//! countryName = optional
//! stateOrProvinceName = optional
//! localityName = optional
//! organizationName = supplied
//! organizationalUnitName = supplied
//! commonName = supplied
//! emailAddress = optional
//!
//! [crl_ext]
//! authorityKeyIdentifier=keyid:always
//!
//! [ usr_cert ]
//! basicConstraints = CA:FALSE
//! keyUsage = digitalSignature, keyEncipherment
//! extendedKeyUsage = clientAuth, serverAuth
//! authorityKeyIdentifier = keyid,issuer
//! subjectKeyIdentifier = hash
//! subjectAltName = $ENV::SAN
//!
//! ```
//!
//! 2. Initialize Directory & Files
//! ```
//! mkdir -p db.certs input output
//! touch db.index
//! echo "01" > db.serial
//! ```
//!
//! 3. Generate CA Private Key & Cert:
//! ```
//! openssl ecparam -name prime256v1 -genkey -noout -out root.key -aes256
//! openssl req -new -x509 -key root.key -out root.crt -days 3650 -sha256
//! ```
//!
//! 4. Generate Certificate Revocation List & Convert to right format:
//! ```
//! openssl ca -config ca.conf -gencrl -out crl.pem
//! openssl crl -in crl.pem -out crl.der -outform DER
//! ```
//!
//! ### Create & Sign Certificates for each Server
//! Repeat for every server.
//! 1. On the Server: Generate a private key & a certificate signing request: (Replace <hostname> with your hostname (or use localhost for testing))
//! ```
//! openssl ecparam -name prime256v1 -genkey -noout -out client.key
//! openssl req -new -key client.key -out client.csr -sha256
//! ```
//! 2. Transfer your .csr File to the computer with the CA certificate
//! 3. Set the SAN & sign with the CA:
//! ```
//! export SAN="DNS:<hostname>"
//! openssl ca -config ca.conf -in client.csr -out client.crt -days 3650 -extensions usr_cert
//! ```
//!
//! # Communication Protocol
//! Main Server -> Rendering Server, establish TCP Connection
//! Main Server -> Rendering Server: [vb_exchange::Message::RenderingRequest]
//! Rendering Server -> Main Server: If template data not saved in current version: [vb_exchange::Message::TemplateDataRequest]
//! Main Server -> Rendering Server: Send Template data (if requested): [vb_exchange::Message::TemplateDataResult]
//! Rendering Server -> Main Server: Send Rendering Status update: [vb_exchange::Message::RenderingRequestStatus]
//! Rendering Server -> Main Server: Send Rendering Result: [vb_exchange::Message::RenderingResult]

use std::fs::{create_dir, remove_dir_all};
use std::path::Path;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_rustls::rustls::server::WebPkiClientVerifier;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;
use crate::settings::Settings;
use vb_exchange::certs::*;
use crate::connection_handler::process_connection;
use crate::rendering::rendering_worker;
use crate::storage::Storage;

pub mod settings;
pub mod storage;
pub mod connection_handler;
pub mod rendering;

#[tokio::main]
async fn main() {
    let settings : Arc<Settings> = Arc::new(Settings::new().expect("Couldn't read config(s)!"));

    // Clear template folder or create if it doesn't exist
    let path = Path::new(&settings.temp_template_path);
    if !path.exists(){
        if let Err(e) = tokio::fs::create_dir(path).await{
            eprintln!("Couldn't create new temp template dir: {}. Check your temp_template_path setting & file permissions.", e);
            return;
        }
    }else {
        if let Err(e) = storage::clear_template_dir(&settings) {
            eprintln!("Couldn't clear template dir: {}", e);
            return;
        }
    }

    // Remove and re-crate temp dir
    let temp_dir_path = Path::new("temp");
    let _ = remove_dir_all(temp_dir_path);
    create_dir(temp_dir_path).unwrap();

    let storage = Arc::new(Storage::new());

    // Load certs
    let root_ca = Arc::new(load_root_ca(settings.ca_cert_path.clone()));
    let client_cert = load_client_cert(settings.client_cert_path.clone());
    let client_key = load_private_key(settings.client_key_path.clone());
    let crls = load_crl(settings.revocation_list_path.clone());

    // Server Config
    let client_verifier = WebPkiClientVerifier::builder(root_ca.clone()).with_crls(crls).build().expect("Couldn't build Client Verifier. Check Certs & Key!");

    let server_config = ServerConfig::builder_with_protocol_versions(&[&tokio_rustls::rustls::version::TLS13])
        .with_client_cert_verifier(client_verifier)
        .with_single_cert(client_cert.clone(), client_key).expect("Couldn't build Server Config. Check Certs & Key!");

    // Create Server to listen on incoming rendering requests
    let acceptor = TlsAcceptor::from(Arc::new(server_config));
    let listener = TcpListener::bind(format!("{}:{}", settings.bind_to_host, settings.port)).await.unwrap();

    // Spawn rendering thread
    let storage_cpy = storage.clone();
    let settings_cpy = settings.clone();
    tokio::spawn(async move{
        println!("Starting rendering worker.");
        rendering_worker(storage_cpy, settings_cpy).await;
    });

    loop{
        let (socket, incoming_address) = match listener.accept().await{
            Ok(res) => res,
            Err(e) => {
                eprintln!("Failed to establish connection: {}", e);
                continue;
            }
        };

        println!("Got an connection from: {}", incoming_address);
        let acceptor = acceptor.clone();

        let storage_cpy = storage.clone();
        let settings_cpy = settings.clone();
        tokio::spawn(async move{
            match acceptor.accept(socket).await{
                Ok(tls_stream) => process_connection(tls_stream.into(), storage_cpy, settings_cpy).await,
                Err(e) => {
                    eprintln!("Failed to accept TLS connection: {}", e);
                }
            }
        });
    }
}


