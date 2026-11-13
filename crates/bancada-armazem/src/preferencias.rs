//! Preferências chave -> JSON, num `escopo` opcional (por exemplo, um perfil local).
//!
//! A tabela entra na escada com [`MIGRACAO_PREFERENCIAS`]; as funções aceitam qualquer
//! `&Connection`, inclusive uma transação (que se converte em `&Connection`).

use crate::erro::Resultado;
use crate::migracoes::Migracao;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Escopo para preferências que não são de ninguém em particular.
pub const ESCOPO_GLOBAL: &str = "";

/// SQL que cria a tabela `preferencias`.
pub const SQL_PREFERENCIAS: &str = "
CREATE TABLE IF NOT EXISTS preferencias (
    escopo     TEXT NOT NULL DEFAULT '',
    chave      TEXT NOT NULL,
    valor_json TEXT NOT NULL,
    PRIMARY KEY (escopo, chave)
) WITHOUT ROWID;
";

/// A tabela de preferências como um degrau pronto da escada.
pub const MIGRACAO_PREFERENCIAS: Migracao =
    Migracao::sql("tabela de preferências", SQL_PREFERENCIAS);

/// Lê a preferência; `None` se nunca foi gravada.
///
/// # Errors
/// [`Erro::Json`](crate::Erro::Json) se o valor gravado não serve para `T`; falha do SQLite.
pub fn ler_preferencia<T: DeserializeOwned>(
    conexao: &Connection,
    escopo: &str,
    chave: &str,
) -> Resultado<Option<T>> {
    let texto: Option<String> = conexao
        .query_row(
            "SELECT valor_json FROM preferencias WHERE escopo = ?1 AND chave = ?2",
            params![escopo, chave],
            |l| l.get(0),
        )
        .optional()?;
    Ok(texto.map(|t| serde_json::from_str(&t)).transpose()?)
}

/// Grava (ou substitui) a preferência; persiste no ato.
///
/// # Errors
/// [`Erro::Json`](crate::Erro::Json) se `valor` não vira JSON; falha do SQLite.
pub fn gravar_preferencia<T: Serialize + ?Sized>(
    conexao: &Connection,
    escopo: &str,
    chave: &str,
    valor: &T,
) -> Resultado<()> {
    conexao.execute(
        "INSERT INTO preferencias (escopo, chave, valor_json) VALUES (?1, ?2, ?3)
         ON CONFLICT (escopo, chave) DO UPDATE SET valor_json = excluded.valor_json",
        params![escopo, chave, serde_json::to_string(valor)?],
    )?;
    Ok(())
}

/// Remove a preferência; `true` se existia.
///
/// # Errors
/// Falha do SQLite.
pub fn remover_preferencia(conexao: &Connection, escopo: &str, chave: &str) -> Resultado<bool> {
    let n = conexao.execute(
        "DELETE FROM preferencias WHERE escopo = ?1 AND chave = ?2",
        params![escopo, chave],
    )?;
    Ok(n > 0)
}
