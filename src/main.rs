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
//! extendedKeyUsage = clientAuth
//! authorityKeyIdentifier = keyid,issuer
//! subjectKeyIdentifier = hash
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
//! 1. On the Server: Generate a private key & a certificate signing request:
//! ```
//! openssl ecparam -name prime256v1 -genkey -noout -out client.key
//! openssl req -new -key client.key -out client.csr -sha256
//! ```
//! 2. Transfer your .csr File to the computer with the CA certificate
//! 3. Sign with the CA:
//! ```
//! openssl ca -config ca.config -in client.csr -out client.crt -days 3650 -extensions usr_cert
//! ```


use std::sync::Arc;
use tokio_rustls::rustls::server::WebPkiClientVerifier;
use tokio_rustls::rustls::{ClientConfig, ServerConfig};
use crate::settings::Settings;

pub mod settings;
pub mod certs;

fn main() {
    let settings : Arc<Settings> = Arc::new(settings::Settings::new().expect("Couldn't read config(s)!"));

    // Load certs
    let root_ca = Arc::new(certs::load_root_ca(&settings));
    let client_cert = certs::load_client_cert(settings.clone());
    let client_key = certs::load_private_key(settings.clone());
    let client_key2 = certs::load_private_key(settings.clone());
    let crls = certs::load_crl(settings.clone());

    // Server Config
    let client_verifier = WebPkiClientVerifier::builder(root_ca.clone()).with_crls(crls).build().expect("Couldn't build Client Verifier. Check Certs & Key!");

    let server_config = ServerConfig::builder_with_protocol_versions(&[&tokio_rustls::rustls::version::TLS13])
        .with_client_cert_verifier(client_verifier)
        .with_single_cert(client_cert.clone(), client_key).expect("Couldn't build Server Config. Check Certs & Key!");

    // Client Config
    let client_config = ClientConfig::builder_with_protocol_versions(&[&tokio_rustls::rustls::version::TLS13])
        .with_root_certificates(root_ca).with_client_auth_cert(client_cert, client_key2).expect("Couldn't build Client Config. Check Certs & Key!");

    println!("Hello, world!");
}