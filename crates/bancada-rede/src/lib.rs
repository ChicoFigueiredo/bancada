//! O portão de rede da bancada, extraído do `chess-net` do Chess Studio: a **única** porta de
//! saída do app para a rede. Síncrono (`ureq` em threads, sem runtime assíncrono), genérico
//! (nada de xadrez nem de finanças), identificadores em português.
//!
//! # O que este crate garante
//!
//! Todo pedido passa por um ponto único (o portão, [`Rede`]), na ordem:
//!
//! 1. **Catálogo**: o endereço precisa ser HTTPS e estar entre os [`Endereco`]s do
//!    [`Catalogo`]. HTTP sem TLS e endereço de fora são recusados **antes** de abrir conexão.
//! 2. **Autorização**: o cabeçalho `Authorization` vem de um [`bancada_segredos::Segredo`]
//!    entregue pela [`FonteDeAutorizacao`] (o chaveiro do app), só para os endereços que o
//!    catálogo marca ([`Endereco::com_autorizacao`]), e nunca aparece em `Debug`, erro ou log.
//!    Pedido que pede autorização nunca sai sem o cabeçalho em silêncio. Um `401`/`403` avisa a
//!    fonte, com o corpo da resposta ([`FonteDeAutorizacao::recusada`]).
//! 3. **Limites**: balde de fichas por host, `429`/`Retry-After` (o host fica "limitado até"),
//!    backoff com jitter para falhas temporárias de `GET` (`POST` nunca se repete sozinho).
//!
//! Depois do portão vêm o cache de respostas com `ETag` em SQLite (opcional,
//! [`ConfiguracaoDaRede::cache_em`]) com leitura offline, o pedido condicional com a `ETag` de
//! quem chama ([`Pedido::se_mudou_desde`], para quem tem o próprio cache), e o estado da rede
//! para a barra de status ([`Rede::estado`]).
//!
//! A única escuta de rede também mora aqui: o servidor de loopback que recebe o retorno da
//! autorização pelo navegador ([`RetornoPorLoopback`]), só em `127.0.0.1`, para um único
//! retorno, com prazo; e o fluxo OAuth2 com PKCE em cima dele ([`FluxoOAuth`]).
//!
//! ```no_run
//! use bancada_rede::{Catalogo, ConfiguracaoDaRede, Endereco, Pedido, Rede};
//!
//! let catalogo = Catalogo::novo()
//!     .com(Endereco::novo("servidor", "https://sbf.exemplo.com.br/api")?.com_autorizacao());
//! let rede = Rede::abrir(ConfiguracaoDaRede::nova(catalogo))?;
//! let mut pendente = rede.buscar(Pedido::get("https://sbf.exemplo.com.br/api/v1/estado"));
//! // A cada quadro (nunca bloqueia):
//! if let Some(resultado) = pendente.tentar_pegar() {
//!     println!("{} bytes", resultado?.corpo.len());
//! }
//! # Ok::<(), bancada_rede::ErroDeRede>(())
//! ```
//!
//! O tempo vem de um [`Relogio`] injetado ([`RelogioFalso`] nos testes); a suíte usa o
//! servidor falso de [`testes`] (feature `testes`) e nunca chama serviço real.

mod autorizacao;
mod cache;
mod catalogo;
mod erro;
mod estado;
mod limites;
mod loopback;
mod navegador;
mod oauth;
mod pedido;
mod portao;
mod relogio;
mod resposta;
mod transporte;

#[cfg(feature = "testes")]
pub mod testes;

pub use autorizacao::{Autorizacao, FonteDeAutorizacao, SemAutorizacao, TAMANHO_MAXIMO_DO_TOKEN};
pub use catalogo::{Catalogo, Endereco};
pub use erro::{CorpoDeErro, ErroDeRede, LIMITE_DO_CORPO_DE_ERRO};
pub use estado::{EstadoDaRede, Operacao, SituacaoDoHost};
pub use limites::{PoliticaDeRepeticao, Ritmo};
pub use loopback::{ErroDoRetorno, ParametrosDoRetorno, RetornoPorLoopback};
pub use navegador::{SistemaDoNavegador, abrir_no_navegador, comando_do_navegador};
pub use oauth::{
    CodigoDeAutorizacao, ErroOAuth, FluxoOAuth, GOOGLE_AUTORIZACAO, GOOGLE_TOKEN, ProvedorOAuth,
    TokensOAuth, desafio_pkce,
};
pub use pedido::{Metodo, Pedido, codificar};
pub use portao::{ConfiguracaoDaRede, Pendente, Rede};
pub use relogio::{Instante, Relogio, RelogioDoSistema, RelogioFalso};
pub use resposta::{Frescor, Resposta};

// Para quem usa o portão não precisar depender da bancada-segredos só por ele.
pub use bancada_segredos::Segredo;
