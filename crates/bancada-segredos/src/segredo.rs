//! [`Segredo`]: um valor secreto em memória.

use std::fmt;
use zeroize::Zeroizing;

/// Um valor secreto (senha, token, chave, CPF). Não tem `Display`; o `Debug` mostra só
/// `Segredo(***)`; a memória é zerada quando o valor é descartado (cada clone zera a sua
/// cópia). Não implementa `PartialEq`, para ninguém comparar segredos em tempo variável.
#[derive(Clone)]
pub struct Segredo(Zeroizing<String>);

impl Segredo {
    /// Guarda `valor` como segredo. O `String` recebido passa a ser zerado ao descartar; uma
    /// cópia que o chamador tenha feito antes não é alcançada.
    pub fn novo(valor: impl Into<String>) -> Self {
        Self(Zeroizing::new(valor.into()))
    }

    /// O valor em claro. Só a guarda (para gravar no keyring, cifrar ou conferir a senha)
    /// e quem de fato usa o segredo devem chamar isto; um teste estático falha se outro arquivo
    /// de produção da bancada o fizer.
    pub fn expor(&self) -> &str {
        &self.0
    }

    /// O segredo está vazio.
    pub fn esta_vazio(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Segredo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Segredo(***)")
    }
}
