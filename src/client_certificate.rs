use openssl::{
    pkey::{PKey, Private},
    x509::X509,
};
use tokio_rustls::rustls::{Certificate, PrivateKey};

pub struct ClientCertificate {
    pub pkey: PrivateKey,
    pub cert: Certificate,
}

impl ClientCertificate {
    pub async fn from_pks12_file(filename: &str, password: &str) -> Self {
        let filename = rust_extensions::file_utils::format_path(filename);
        let content = tokio::fs::read(filename.as_str()).await.unwrap();
        Self::from_pkcs12(&content, password)
    }
    pub fn from_pkcs12(src: &[u8], password: &str) -> Self {
        let pkcs12 = openssl::pkcs12::Pkcs12::from_der(src)
            .unwrap()
            .parse2(password)
            .unwrap();

        let pkey: PKey<Private> = pkcs12.pkey.unwrap();
        let cert: X509 = pkcs12.cert.unwrap();

        Self {
            pkey: PrivateKey(pkey.private_key_to_der().unwrap()),
            cert: Certificate(cert.to_der().unwrap()),
        }
    }

    pub fn clone(&self) -> Self {
        Self {
            pkey: self.pkey.clone(),
            cert: self.cert.clone(),
        }
    }
}
