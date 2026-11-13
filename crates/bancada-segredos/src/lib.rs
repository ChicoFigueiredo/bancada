//! Guarda de segredos compartilhada entre o Chess Studio e o Seu Bolso Feliz desktop (ADR-018
//! do SBF), extraída do `chess-vault`. Genérica: rótulos de texto, nome do serviço como
//! parâmetro, nada de xadrez nem de finanças. Sem interface gráfica.
//!
//! - **Keyring do sistema operacional primeiro** ([`ChaveiroNativo`]): Secret Service no Linux
//!   (por `zbus`, Rust puro), Credential Manager no Windows, Keychain no macOS. Fica atrás do
//!   trait [`ChaveiroDoSistema`]; a suíte usa um falso em memória (feature `testes`) e nunca
//!   toca o keyring real nem abre diálogo do SO.
//! - **Sem keyring, arquivo cifrado por senha-mestra**: Argon2id + XChaCha20-Poly1305, formato
//!   versionado (abaixo), criado pelo usuário ([`GuardaDeSegredos::criar_arquivo`]),
//!   destrancado só quando preciso, trancável, com troca de senha. **Nunca** texto puro em
//!   disco e **nunca** troca de lugar em silêncio: sem nenhum dos dois, a guarda fica
//!   [`Situacao::Indisponivel`] e recusa gravar.
//! - **Interface única**: situação, gravar, ler, apagar e listar rótulos (sem valores).
//!   [`Segredo`] não tem `Display`, o `Debug` é `Segredo(***)`, e o que é secreto é zerado na
//!   memória ao descartar. [`ErroDeSegredo`] nunca carrega valor, senha nem rótulo, e senha
//!   errada e arquivo adulterado dão o mesmo erro.
//!
//! ```no_run
//! use bancada_segredos::{ConfiguracaoDaGuarda, GuardaDeSegredos, Segredo, Situacao};
//!
//! let configuracao = ConfiguracaoDaGuarda::nova("Seu Bolso Feliz", "dados/chaveiro");
//! let guarda = GuardaDeSegredos::abrir(configuracao)?;
//! if guarda.situacao() == Situacao::Indisponivel {
//!     // A interface explica e pede uma senha-mestra nova.
//!     guarda.criar_arquivo(&Segredo::novo("uma senha longa"))?;
//! }
//! guarda.gravar("senha:banco", Segredo::novo("valor"))?;
//! let valor = guarda.ler("senha:banco")?;
//! # let _ = valor;
//! # Ok::<(), bancada_segredos::ErroDeSegredo>(())
//! ```
//!
//! # No disco
//!
//! No diretório da guarda (criado 0700 no Unix), dois arquivos, ambos 0600 no Unix e gravados
//! de forma atômica (temporário novo + `fsync` + renomear):
//!
//! - `rotulos.json`: o índice, só com os rótulos (`{"formato":1,"rotulos":[…]}`), para listar
//!   com o arquivo trancado ou com o keyring. **Os rótulos ficam em claro**: não ponha dado
//!   sensível no rótulo.
//! - o arquivo cifrado (`segredos.cifrado` por padrão; configurável), só no modo sem keyring.
//!
//! # Formato do arquivo cifrado (inteiros em little-endian)
//!
//! | Bytes | Campo |
//! |---|---|
//! | 0..8 | assinatura `CSVAULT\0` (a do chess-vault, mantida) |
//! | 8..10 | versão do formato: `1` (chess-vault, só leitura) ou `2` (bancada, gravado) |
//! | 10 | KDF: `1` = Argon2id (versão 0x13) |
//! | 11..23 | memória (KiB), iterações e paralelismo do Argon2id (`u32` cada) |
//! | 23..39 | sal (16 bytes aleatórios, novo a cada troca de senha) |
//! | 39..71 | verificador: os 32 primeiros bytes do Argon2id |
//! | 71 | cifra: `1` = XChaCha20-Poly1305 |
//! | 72..96 | nonce (24 bytes aleatórios, novo a cada gravação) |
//! | 96.. | conteúdo cifrado + etiqueta de 16 bytes |
//!
//! O Argon2id da senha com o sal dá 64 bytes: os 32 primeiros são o verificador e os 32
//! últimos, a chave. O cabeçalho inteiro (bytes 0..96, com a versão) é o dado associado (AAD)
//! da cifra: mudar qualquer byte dele faz a leitura falhar. Ao destrancar, o verificador é
//! conferido em tempo constante **e** a decifração é tentada; qualquer das duas falhando dá
//! [`ErroDeSegredo::SenhaOuArquivoInvalido`]. Só a versão mais nova que a conhecida tem erro
//! próprio ([`ErroDeSegredo::FormatoNaoSuportado`]).
//!
//! O cabeçalho é idêntico nas duas versões; muda o conteúdo decifrado:
//!
//! - **Versão 1** (chess-vault): JSON
//!   `{"entries":[{"connection":7,"provider":"lichess","profile":1,"secret":"…"}]}`. Cada
//!   entrada vira o rótulo `provider/profile/connection` (a conta que o chess-vault usava no
//!   keyring). Lido sem conversão; só é regravado (já na versão 2) quando algo muda.
//! - **Versão 2** (bancada): binário, `u32` quantidade e, por segredo, em ordem de rótulo,
//!   `u32` + rótulo UTF-8 e `u32` + valor UTF-8, sem sobra no fim. Binário de tamanho exato
//!   para que nenhum buffer com valor em claro seja realocado.
//!
//! Um Chess Studio antigo, com o chess-vault, recusa a versão 2 como "formato mais novo": a
//! migração é de mão única.

mod arquivo_cifrado;
mod chaveiro;
mod conteudo;
mod erro;
mod gravacao;
mod guarda;
mod indice;
mod rotulo;
mod segredo;

#[cfg(feature = "testes")]
pub mod testes;

pub use arquivo_cifrado::ParametrosDeDerivacao;
pub use chaveiro::{ChaveiroDoSistema, ChaveiroNativo};
pub use erro::{ErroDeSegredo, FalhaDoChaveiro, TAMANHO_MINIMO_DA_SENHA};
pub use guarda::{
    ARQUIVO_CIFRADO_PADRAO, ARQUIVO_DO_INDICE, ConfiguracaoDaGuarda, GuardaDeSegredos, Situacao,
};
pub use rotulo::TAMANHO_MAXIMO_DO_ROTULO;
pub use segredo::Segredo;
