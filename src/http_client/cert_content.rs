use std::sync::Arc;

lazy_static::lazy_static! {
    pub static ref ROOT_CERT_STORE: Arc<tokio_rustls::rustls::RootCertStore> = {
        let mut root_cert_store = tokio_rustls::rustls::RootCertStore::empty();

        root_cert_store.add_parsable_certificates(get_der_certificates().as_slice());

        Arc::new(root_cert_store)
    };
}

fn get_der_certificates() -> Vec<Vec<u8>> {
    let mut result = Vec::new();

    let mut iterator = split_certificates().into_iter();

    while let Some(itm) = iterator.next() {
        result.push(pem_to_der(itm.as_slice()));
    }

    result
}

fn split_certificates() -> Vec<Vec<u8>> {
    let mut result = Vec::new();

    let mut cert = None;

    let mut iterator = std::str::from_utf8(ALL_CERTIFICATES)
        .unwrap()
        .split("\n")
        .into_iter();

    while let Some(itm) = iterator.next() {
        if itm == "-----BEGIN CERTIFICATE-----" {
            cert = Some(Vec::new());
            cert.as_mut().unwrap().extend_from_slice(itm.as_bytes());
            cert.as_mut().unwrap().push(b'\n');
            continue;
        } else if itm == "-----END CERTIFICATE-----" {
            cert.as_mut().unwrap().extend_from_slice(itm.as_bytes());
            cert.as_mut().unwrap().push(b'\n');

            if let Some(cert_to_add) = cert.take() {
                result.push(cert_to_add);
            }
            continue;
        }

        if let Some(cert) = cert.as_mut() {
            cert.extend_from_slice(itm.as_bytes());
            cert.push(b'\n');
        }
    }

    result
}

pub static ALL_CERTIFICATES: &'static [u8] = std::include_bytes!("cacert-2023-08-22.pem");

pub fn pem_to_der(pem_data: &[u8]) -> Vec<u8> {
    use pem::parse;
    // Parse the PEM file
    let pem = parse(pem_data).unwrap();

    // The pem::Pem struct contains the decoded data
    pem.contents().to_vec()
}

#[cfg(test)]
mod tests {
    use super::split_certificates;

    #[test]
    fn test_split_certs() {
        let certs = split_certificates();

        for cert in &certs {
            println!("{}", std::str::from_utf8(cert).unwrap());
            println!("--------")
        }
    }
}
