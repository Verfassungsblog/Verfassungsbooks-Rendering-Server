use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;
use tokio_rustls::rustls::pki_types::{CertificateDer, CertificateRevocationListDer, PrivateKeyDer};
use tokio_rustls::rustls::RootCertStore;
use crate::settings::Settings;

pub fn load_root_ca(settings: &Arc<Settings>) -> RootCertStore {
    // Load certificates
    let mut root_store = tokio_rustls::rustls::RootCertStore::empty();
    let ca_file = File::open(&settings.ca_cert_path).expect("cannot open CA file");
    let mut reader = BufReader::new(ca_file);
    let certs: Vec<_> = rustls_pemfile::certs(&mut reader).map(|cert| cert.expect("Couldn't parse root CA")).collect();
    for cert in certs{
        root_store.add(cert).expect("Couldn't add CA file to root store.");
    }
    root_store
}

pub fn load_client_cert(settings: Arc<Settings>) -> Vec<CertificateDer<'static>>{
    let file = File::open(&settings.client_cert_path).expect("cannot open client cert file");
    let mut reader = BufReader::new(file);
    let certs = rustls_pemfile::certs(&mut reader);
    let certs = certs.map(|cert|cert.expect("Couldn't parse cert file")).collect();
    certs
}

pub fn load_private_key(settings: Arc<Settings>) -> PrivateKeyDer<'static>{
    let file = File::open(&settings.client_key_path).expect("cannot open client key file");
    let mut reader = BufReader::new(file);
    rustls_pemfile::private_key(&mut reader).expect("Couldn't parse Private key file!").expect("Missing private key")
}

pub fn load_crl(settings: Arc<Settings>) -> Vec<CertificateRevocationListDer<'static>>{
    let crl_file = File::open(settings.revocation_list_path.clone()).expect("Failed to open CRL file");
    let mut crl_reader = BufReader::new(crl_file);
    let res = rustls_pemfile::crls(&mut crl_reader).map(|cert|cert.expect("Couldn't load CRL!")).collect();

    res
}