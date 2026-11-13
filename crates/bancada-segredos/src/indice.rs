//! O índice de rótulos (`rotulos.json`): quais rótulos têm segredo guardado. **Nenhum
//! valor**: lista-se mesmo com o arquivo cifrado trancado. Não é a fonte da verdade: ler vai
//! direto ao keyring ou ao conteúdo decifrado, e destrancar o arquivo regrava o índice a partir
//! do conteúdo.

use crate::erro::ErroDeSegredo;
use crate::gravacao::gravar_atomicamente;
use crate::rotulo;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

const FORMATO: u16 = 1;

#[derive(Serialize, Deserialize)]
struct EmDisco {
    formato: u16,
    rotulos: BTreeSet<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Indice {
    pub(crate) rotulos: BTreeSet<String>,
}

impl Indice {
    pub(crate) fn carregar(caminho: &Path) -> Result<Self, ErroDeSegredo> {
        let bytes = match fs::read(caminho) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(ErroDeSegredo::disco(&e)),
        };
        let lido: EmDisco =
            serde_json::from_slice(&bytes).map_err(|_| ErroDeSegredo::IndiceCorrompido)?;
        if lido.formato > FORMATO {
            return Err(ErroDeSegredo::FormatoNaoSuportado {
                versao: lido.formato,
            });
        }
        if lido.formato < FORMATO || lido.rotulos.iter().any(|r| rotulo::validar(r).is_err()) {
            return Err(ErroDeSegredo::IndiceCorrompido);
        }
        Ok(Self {
            rotulos: lido.rotulos,
        })
    }

    pub(crate) fn salvar(&self, caminho: &Path) -> Result<(), ErroDeSegredo> {
        let bytes = serde_json::to_vec_pretty(&EmDisco {
            formato: FORMATO,
            rotulos: self.rotulos.clone(),
        })
        .map_err(|_| ErroDeSegredo::Interno {
            detalhe: "índice ilegível",
        })?;
        gravar_atomicamente(caminho, &bytes)
    }
}
