//! Tabela só de acréscimo: triggers que recusam `UPDATE` e `DELETE`.
//!
//! Serve a logs de decisões e de eventos: a história só cresce. Os triggers não impedem
//! `DROP TABLE` nem a troca do arquivo inteiro; são uma barreira contra o código do próprio app,
//! não contra quem tem o banco na mão.
//!
//! `INSERT OR REPLACE` só dispara o trigger de `DELETE` com `recursive_triggers` ligado, o que
//! [`Pragmas`](crate::Pragmas) faz por padrão. Uma conexão aberta por fora (um script, o
//! `sqlite3`) com o PRAGMA desligado consegue o REPLACE.

use crate::erro::{Erro, Resultado};
use rusqlite::Connection;

fn validar_nome(tabela: &str) -> Resultado<()> {
    let mut caracteres = tabela.chars();
    let valido = caracteres
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && caracteres.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if valido {
        Ok(())
    } else {
        Err(Erro::NomeInvalido(tabela.to_owned()))
    }
}

fn nome_do_trigger(tabela: &str, operacao: &str) -> String {
    format!("{tabela}_recusa_{operacao}")
}

/// SQL que cria (se ainda não existirem) os dois triggers de `tabela`. Use dentro de uma
/// [`Migracao::codigo`](crate::Migracao::codigo) ou passe a [`Connection::execute_batch`].
///
/// # Errors
/// [`Erro::NomeInvalido`] se `tabela` não for um identificador simples.
pub fn gerar_sql_somente_acrescimo(tabela: &str) -> Resultado<String> {
    validar_nome(tabela)?;
    let mut sql = String::new();
    for (operacao, verbo) in [("update", "UPDATE"), ("delete", "DELETE")] {
        let gatilho = nome_do_trigger(tabela, operacao);
        sql.push_str(&format!(
            "CREATE TRIGGER IF NOT EXISTS \"{gatilho}\" BEFORE {verbo} ON \"{tabela}\"\n\
             BEGIN\n    \
             SELECT RAISE(ABORT, 'a tabela {tabela} é somente de acréscimo: {verbo} recusado');\n\
             END;\n"
        ));
    }
    Ok(sql)
}

/// Instala os triggers em `tabela` (que precisa existir). Repetir é inofensivo.
///
/// # Errors
/// [`Erro::NomeInvalido`], ou falha do SQLite (por exemplo, a tabela não existe).
pub fn instalar_somente_acrescimo(conexao: &Connection, tabela: &str) -> Resultado<()> {
    conexao.execute_batch(&gerar_sql_somente_acrescimo(tabela)?)?;
    Ok(())
}

/// `true` se os dois triggers de `tabela` estão instalados.
///
/// # Errors
/// [`Erro::NomeInvalido`] ou falha do SQLite.
pub fn esta_protegida(conexao: &Connection, tabela: &str) -> Resultado<bool> {
    validar_nome(tabela)?;
    let n: i64 = conexao.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND tbl_name = ?1 AND name IN (?2, ?3)",
        [
            tabela,
            &nome_do_trigger(tabela, "update"),
            &nome_do_trigger(tabela, "delete"),
        ],
        |l| l.get(0),
    )?;
    Ok(n == 2)
}
