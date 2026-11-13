//! Mecânica de SQLite compartilhada entre o Chess Studio e o Seu Bolso Feliz desktop.
//!
//! Nada aqui sabe de xadrez nem de finanças: só abrir o banco com os PRAGMAs certos, migrar por
//! `PRAGMA user_version`, gravar em lotes curtos, proteger tabelas só de acréscimo, copiar o
//! banco com segurança e guardar preferências.
//!
//! ```
//! use bancada_armazem::{Armazem, Erro, Escada, Migracao, Pragmas};
//!
//! const ESCADA: &[Migracao] = &[Migracao::sql("pessoa", "CREATE TABLE pessoa (nome TEXT NOT NULL);")];
//!
//! # fn main() -> Result<(), Erro> {
//! let mut armazem = Armazem::em_memoria(&Pragmas::default())?;
//! armazem.migrar(&Escada::nova(ESCADA))?;
//! armazem.transacao_imediata(|tx| -> Result<(), Erro> {
//!     tx.execute("INSERT INTO pessoa (nome) VALUES ('Ana')", [])?;
//!     Ok(())
//! })?;
//! # Ok(())
//! # }
//! ```
//!
//! Regras de concorrência (vários processos no mesmo arquivo): WAL, `espera_ocupado` longa,
//! lotes curtos em [`transacao_imediata`] e leitura pela conexão separada. Nenhuma transação
//! atravessa uma espera.

mod armazem;
mod copia;
mod erro;
mod migracoes;
mod pragmas;
mod preferencias;
mod somente_acrescimo;
mod transacao;

pub use armazem::Armazem;
pub use copia::{copiar_banco, verificar_integridade, verificar_integridade_de_arquivo};
pub use erro::{Erro, Resultado};
pub use migracoes::{
    Escada, LinhaDeBase, MarcadorDeOrigem, Migracao, RelatorioMigracao, migrar, migrar_ate,
    pendentes, versao_do_banco,
};
pub use pragmas::{ModoDiario, Pragmas, PragmasLidos, Sincronizacao, ler_pragmas};
pub use preferencias::{
    ESCOPO_GLOBAL, MIGRACAO_PREFERENCIAS, SQL_PREFERENCIAS, gravar_preferencia, ler_preferencia,
    remover_preferencia,
};
pub use somente_acrescimo::{
    esta_protegida, gerar_sql_somente_acrescimo, instalar_somente_acrescimo,
};
pub use transacao::transacao_imediata;
