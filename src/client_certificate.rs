use openssl::{
    pkey::{PKey, Private},
    x509::X509,
};

pub struct ClientCertificate {
    pub pkey: rustls::PrivateKey,
    pub cert: rustls::Certificate,
}

impl ClientCertificate {
    pub fn from_pkcs12(src: &[u8], password: &str) -> Self {
        let pkcs12 = openssl::pkcs12::Pkcs12::from_der(src)
            .unwrap()
            .parse2(password)
            .unwrap();

        let pkey: PKey<Private> = pkcs12.pkey.unwrap();
        let cert: X509 = pkcs12.cert.unwrap();

        Self {
            pkey: rustls::PrivateKey(pkey.private_key_to_der().unwrap()),
            cert: rustls::Certificate(cert.to_der().unwrap()),
        }
    }

    pub fn clone(&self) -> Self {
        Self {
            pkey: self.pkey.clone(),
            cert: self.cert.clone(),
        }
    }
}
