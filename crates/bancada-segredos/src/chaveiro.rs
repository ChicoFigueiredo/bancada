//! O keyring do sistema operacional, atrás de um trait ([`ChaveiroDoSistema`]) para que a
//! suíte use um falso em memória (feature `testes`) e nunca grave no keyring real do usuário.
//!
//! A implementação real ([`ChaveiroNativo`]) usa a crate `keyring` (MIT OR Apache-2.0), a
//! mesma do chess-vault: Secret Service por `zbus` no Linux (Rust puro: não exige `libdbus`),
//! Credential Manager no Windows e Keychain no macOS.

use crate::erro::FalhaDoChaveiro;
use crate::segredo::Segredo;
use std::fmt;
use zeroize::Zeroize;

/// Um keyring do sistema: guarda um valor por (serviço, rótulo).
pub trait ChaveiroDoSistema: Send + Sync + fmt::Debug {
    /// O keyring existe e responde, sem gravar nada e sem pedir desbloqueio ao usuário.
    fn esta_disponivel(&self) -> bool;
    /// Grava (ou substitui) o valor do rótulo no serviço.
    fn gravar(&self, servico: &str, rotulo: &str, valor: &Segredo) -> Result<(), FalhaDoChaveiro>;
    /// `Ok(None)` se o rótulo não tem valor.
    fn ler(&self, servico: &str, rotulo: &str) -> Result<Option<Segredo>, FalhaDoChaveiro>;
    /// Apagar o que não existe não é erro.
    fn apagar(&self, servico: &str, rotulo: &str) -> Result<(), FalhaDoChaveiro>;
}

/// O keyring de verdade do sistema operacional.
#[derive(Debug, Clone, Copy, Default)]
pub struct ChaveiroNativo;

/// Traduz o erro do keyring numa falha de mensagem fixa e zera os bytes que ele carregue.
fn falha(erro: keyring::Error) -> FalhaDoChaveiro {
    match erro {
        keyring::Error::NoDefaultStore => FalhaDoChaveiro::Indisponivel,
        keyring::Error::NoStorageAccess(_) => FalhaDoChaveiro::AcessoNegado,
        keyring::Error::TooLong(..) => FalhaDoChaveiro::LongoDemais,
        keyring::Error::BadEncoding(mut bytes) | keyring::Error::BadDataFormat(mut bytes, _) => {
            bytes.zeroize();
            FalhaDoChaveiro::Ilegivel
        }
        _ => FalhaDoChaveiro::Outra,
    }
}

fn entrada(servico: &str, rotulo: &str) -> Result<keyring::Entry, FalhaDoChaveiro> {
    keyring::Entry::new(servico, rotulo).map_err(falha)
}

impl ChaveiroDoSistema for ChaveiroNativo {
    fn esta_disponivel(&self) -> bool {
        // Só a inicialização do keyring (uma vez por processo): não lê nem grava entrada, então
        // não abre diálogo de desbloqueio.
        keyring::Entry::store_status().is_ok()
    }

    fn gravar(&self, servico: &str, rotulo: &str, valor: &Segredo) -> Result<(), FalhaDoChaveiro> {
        entrada(servico, rotulo)?
            .set_password(valor.expor())
            .map_err(falha)
    }

    fn ler(&self, servico: &str, rotulo: &str) -> Result<Option<Segredo>, FalhaDoChaveiro> {
        match entrada(servico, rotulo)?.get_password() {
            Ok(valor) => Ok(Some(Segredo::novo(valor))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(erro) => Err(falha(erro)),
        }
    }

    fn apagar(&self, servico: &str, rotulo: &str) -> Result<(), FalhaDoChaveiro> {
        match entrada(servico, rotulo)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(erro) => Err(falha(erro)),
        }
    }
}
