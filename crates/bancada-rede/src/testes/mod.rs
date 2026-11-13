//! Apoio de teste (feature `testes`, só em `[dev-dependencies]`): servidor HTTP falso em
//! porta efêmera de loopback. Com `ConfiguracaoDaRede::rota`, o host oficial vai para o
//! servidor falso; a suíte nunca chama serviço real.
//!
//! ```
//! use bancada_rede::testes::{RespostaFalsa, ServidorFalso};
//! use bancada_rede::{Catalogo, ConfiguracaoDaRede, Endereco, Pedido, Rede};
//!
//! let servidor = ServidorFalso::iniciar();
//! servidor.rota("/api/estado", RespostaFalsa::json(r#"{"ok":true}"#));
//! let catalogo = Catalogo::novo().com(Endereco::novo("api", "https://sbf.exemplo/api")?);
//! let rede = Rede::abrir(
//!     ConfiguracaoDaRede::nova(catalogo).rota("sbf.exemplo", &servidor.base()),
//! )?;
//! let resposta = rede.executar(&Pedido::get("https://sbf.exemplo/api/estado"))?;
//! assert_eq!(resposta.corpo, br#"{"ok":true}"#);
//! assert_eq!(servidor.pedidos()[0].caminho, "/api/estado");
//! # Ok::<(), bancada_rede::ErroDeRede>(())
//! ```

mod servidor;

pub use servidor::{PedidoRecebido, RespostaFalsa, ServidorFalso};
