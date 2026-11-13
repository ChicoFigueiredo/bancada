//! Transação IMMEDIATE curta.
//!
//! A forma é de propósito um fecho: a transação não existe fora dele, então não há como deixá-la
//! aberta atravessando uma espera (um quadro da interface, um `await`, outro processo). Pela
//! lição do inventário, com outro processo gravando, uma leitura dentro de transação aberta fixa
//! um snapshot e a escrita seguinte falha com `SQLITE_BUSY_SNAPSHOT`, que o `busy_timeout` não
//! resolve. Junte o que precisa em memória e grave num lote curto.

use rusqlite::{Connection, Transaction, TransactionBehavior};

/// Abre uma transação IMMEDIATE (pega o lock de escrita já no início, sem upgrade de leitura),
/// roda `corpo` e termina sempre:
///
/// - `Ok` do corpo: `COMMIT`;
/// - `Err` do corpo, ou pânico: `ROLLBACK`.
///
/// Quando a função devolve, a conexão está de volta ao modo automático.
///
/// # Errors
/// O erro do `corpo`, ou falha do SQLite ao abrir ou confirmar (convertida em `E`).
pub fn transacao_imediata<T, E>(
    conexao: &mut Connection,
    corpo: impl FnOnce(&Transaction<'_>) -> Result<T, E>,
) -> Result<T, E>
where
    E: From<rusqlite::Error>,
{
    let tx = conexao.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let resultado = match corpo(&tx) {
        Ok(valor) => tx.commit().map(|()| valor).map_err(E::from),
        Err(erro) => {
            // A falha do rollback não esconde o erro do chamador; o guarda abaixo cuida do resto.
            let _ = tx.rollback();
            Err(erro)
        }
    };
    if !conexao.is_autocommit() {
        // Cinto e suspensório: nunca devolver a conexão com transação pendurada.
        let _ = conexao.execute_batch("ROLLBACK");
    }
    resultado
}
