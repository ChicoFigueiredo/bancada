//! Conferências de URL que o navegador e os roteiros fazem: o host, se é navegável e se a
//! página aceita receber um segredo.

/// A URL é de `http` ou `https`, com um host? Roteiro nenhum navega para `file:`, `data:`,
/// `javascript:`, `chrome:` nem `about:`.
#[must_use]
pub fn url_navegavel(url: &str) -> bool {
    let u = url.trim().to_ascii_lowercase();
    (u.starts_with("http://") || u.starts_with("https://")) && host_de(&u).is_some()
}

/// O host de uma URL `http(s)`, em minúsculas, sem porta nem credenciais. `None` se não é uma
/// URL de rede (`about:blank`, `chrome-error:`…).
#[must_use]
pub fn host_de(url: &str) -> Option<String> {
    let resto = url.split_once("://")?.1;
    let autoridade = resto.split(['/', '?', '#']).next()?;
    let sem_credenciais = autoridade.rsplit('@').next()?;
    let host = if let Some(v6) = sem_credenciais.strip_prefix('[') {
        v6.split(']').next()?
    } else {
        sem_credenciais.split(':').next()?
    };
    let host = host.trim().to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

/// Segredo (senha, CPF) só vai a uma página `https`; `http` só no próprio computador
/// (`127.0.0.1`, `localhost`, `::1`: o site falso dos testes e da demonstração).
#[must_use]
pub fn esquema_aceito_para_segredo(url: &str) -> bool {
    let u = url.trim_start().to_ascii_lowercase();
    if u.starts_with("https://") {
        return host_de(&u).is_some();
    }
    u.starts_with("http://")
        && matches!(
            host_de(&u).as_deref(),
            Some("127.0.0.1" | "localhost" | "::1")
        )
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn host_de_url() {
        assert_eq!(
            host_de("https://Banco.Exemplo.com.br:8443/a?b#c").as_deref(),
            Some("banco.exemplo.com.br")
        );
        assert_eq!(
            host_de("http://u:p@127.0.0.1:80/x").as_deref(),
            Some("127.0.0.1")
        );
        assert_eq!(host_de("http://[::1]:9/x").as_deref(), Some("::1"));
        assert_eq!(host_de("about:blank"), None);
        assert_eq!(
            host_de("chrome-error://chromewebdata/"),
            Some("chromewebdata".into())
        );
        assert_eq!(
            host_de("https://evil.example.com@good.example/").as_deref(),
            Some("good.example")
        );
    }

    #[test]
    fn segredo_so_em_https_ou_no_proprio_computador() {
        assert!(esquema_aceito_para_segredo("https://banco.example/login"));
        assert!(esquema_aceito_para_segredo("http://127.0.0.1:8080/login"));
        assert!(esquema_aceito_para_segredo("http://localhost:3000/"));
        assert!(!esquema_aceito_para_segredo("http://banco.example/login"));
        assert!(!esquema_aceito_para_segredo("ftp://banco.example/"));
        assert!(!esquema_aceito_para_segredo("about:blank"));
        assert!(!esquema_aceito_para_segredo(
            "chrome-error://chromewebdata/"
        ));
        assert!(!esquema_aceito_para_segredo(""));
    }

    #[test]
    fn so_http_e_https_sao_navegaveis() {
        assert!(url_navegavel("https://a.example/x"));
        assert!(url_navegavel("HTTP://127.0.0.1:1/"));
        for u in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,x",
            "chrome://settings",
            "about:blank",
            "https://",
            "",
        ] {
            assert!(!url_navegavel(u), "{u}");
        }
    }
}
