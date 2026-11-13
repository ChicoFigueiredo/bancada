//! Abrir uma página no navegador do sistema. O app não abre a conexão: entrega o endereço ao
//! navegador do usuário. Só `https://` simples (sem espaço nem caractere de controle).

/// O sistema operacional, para escolher o programa que abre o navegador.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SistemaDoNavegador {
    Linux,
    MacOs,
    Windows,
}

impl SistemaDoNavegador {
    /// O sistema em que o app está rodando (outro Unix conta como Linux).
    pub fn atual() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }
}

/// Programa e argumentos que abrem `url` no navegador padrão, ou `None` se o endereço não é
/// um `https://` simples. O endereço vai como argumento próprio, nunca por um shell.
pub fn comando_do_navegador(
    sistema: SistemaDoNavegador,
    url: &str,
) -> Option<(&'static str, Vec<String>)> {
    let simples =
        url.starts_with("https://") && !url.chars().any(|c| c.is_whitespace() || c.is_control());
    if !simples {
        return None;
    }
    Some(match sistema {
        SistemaDoNavegador::Linux => ("xdg-open", vec![url.to_owned()]),
        SistemaDoNavegador::MacOs => ("open", vec![url.to_owned()]),
        SistemaDoNavegador::Windows => (
            "rundll32",
            vec!["url.dll,FileProtocolHandler".to_owned(), url.to_owned()],
        ),
    })
}

/// Pede ao sistema que abra `url` no navegador. Devolve se o pedido foi entregue.
pub fn abrir_no_navegador(url: &str) -> bool {
    let Some((programa, argumentos)) = comando_do_navegador(SistemaDoNavegador::atual(), url)
    else {
        return false;
    };
    let iniciado = std::process::Command::new(programa)
        .args(argumentos)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    match iniciado {
        Ok(mut filho) => {
            // Recolhe o processo filho sem bloquear a interface.
            std::thread::spawn(move || {
                let _ = filho.wait();
            });
            true
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn cada_sistema_tem_o_seu_programa() {
        let url = "https://accounts.exemplo.com/auth?x=1";
        assert_eq!(
            comando_do_navegador(SistemaDoNavegador::Linux, url),
            Some(("xdg-open", vec![url.to_owned()]))
        );
        assert_eq!(
            comando_do_navegador(SistemaDoNavegador::MacOs, url),
            Some(("open", vec![url.to_owned()]))
        );
        let (programa, args) =
            comando_do_navegador(SistemaDoNavegador::Windows, url).expect("windows");
        assert_eq!(programa, "rundll32");
        assert_eq!(args.last().map(String::as_str), Some(url));
    }

    #[test]
    fn so_https_simples_e_aberto() {
        for ruim in [
            "http://exemplo.org",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "https://a b",
            "https://a\nb",
            "",
        ] {
            assert_eq!(
                comando_do_navegador(SistemaDoNavegador::Linux, ruim),
                None,
                "{ruim:?}"
            );
            assert!(!abrir_no_navegador(ruim));
        }
    }
}
