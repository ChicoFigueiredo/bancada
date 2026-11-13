//! O que vai dentro do envelope cifrado: o mapa rótulo → valor.
//!
//! - **Formato 2 (bancada, o que se grava):** binário, com tamanhos exatos, para que nenhum
//!   buffer com valor em claro seja realocado (e deixe cópia na memória liberada):
//!   `u32 quantidade`, e para cada segredo, em ordem de rótulo, `u32 tamanho do rótulo`,
//!   rótulo em UTF-8, `u32 tamanho do valor`, valor em UTF-8 (inteiros em little-endian). Nada
//!   pode sobrar depois do último segredo; rótulo repetido ou inválido recusa o arquivo.
//! - **Formato 1 (chess-vault, só leitura):** JSON
//!   `{"entries":[{"connection":7,"provider":"lichess","profile":1,"secret":"…"}]}`. Cada
//!   entrada vira o rótulo `provider/profile/connection`, a mesma conta que o chess-vault usava
//!   no keyring do sistema.

use crate::arquivo_cifrado::{VERSAO_1, VERSAO_2};
use crate::erro::ErroDeSegredo;
use crate::rotulo;
use crate::segredo::Segredo;
use serde::{Deserialize, Deserializer};
use std::collections::BTreeMap;
use zeroize::Zeroizing;

/// Os segredos de um arquivo destrancado, por rótulo.
pub(crate) type Mapa = BTreeMap<String, Segredo>;

fn tamanho_u32(n: usize) -> Result<[u8; 4], ErroDeSegredo> {
    u32::try_from(n)
        .map(u32::to_le_bytes)
        .map_err(|_| ErroDeSegredo::Interno {
            detalhe: "segredo grande demais",
        })
}

/// Codifica o mapa no formato 2, num buffer de capacidade exata e zerado ao descartar.
pub(crate) fn codificar(mapa: &Mapa) -> Result<Zeroizing<Vec<u8>>, ErroDeSegredo> {
    let total = mapa
        .iter()
        .try_fold(4usize, |acc, (r, v)| {
            acc.checked_add(8)?
                .checked_add(r.len())?
                .checked_add(v.expor().len())
        })
        .ok_or(ErroDeSegredo::Interno {
            detalhe: "segredo grande demais",
        })?;
    let mut saida = Zeroizing::new(Vec::with_capacity(total));
    saida.extend_from_slice(&tamanho_u32(mapa.len())?);
    for (rotulo, valor) in mapa {
        saida.extend_from_slice(&tamanho_u32(rotulo.len())?);
        saida.extend_from_slice(rotulo.as_bytes());
        saida.extend_from_slice(&tamanho_u32(valor.expor().len())?);
        saida.extend_from_slice(valor.expor().as_bytes());
    }
    debug_assert_eq!(saida.len(), total);
    Ok(saida)
}

/// Lê `n` bytes a partir de `*pos`.
fn fatia<'a>(bytes: &'a [u8], pos: &mut usize, n: usize) -> Option<&'a [u8]> {
    let fim = pos.checked_add(n)?;
    let pedaco = bytes.get(*pos..fim)?;
    *pos = fim;
    Some(pedaco)
}

fn u32_lido(bytes: &[u8], pos: &mut usize) -> Option<usize> {
    let b = fatia(bytes, pos, 4)?;
    usize::try_from(u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok()
}

fn decodificar_v2(bytes: &[u8]) -> Option<Mapa> {
    let mut pos = 0;
    let quantidade = u32_lido(bytes, &mut pos)?;
    let mut mapa = Mapa::new();
    for _ in 0..quantidade {
        let n = u32_lido(bytes, &mut pos)?;
        let rotulo = std::str::from_utf8(fatia(bytes, &mut pos, n)?).ok()?;
        rotulo::validar(rotulo).ok()?;
        let n = u32_lido(bytes, &mut pos)?;
        let valor = std::str::from_utf8(fatia(bytes, &mut pos, n)?).ok()?;
        if valor.is_empty() || mapa.contains_key(rotulo) {
            return None;
        }
        // Capacidade exata: a única cópia do valor fora do buffer decifrado.
        let mut texto = String::with_capacity(valor.len());
        texto.push_str(valor);
        mapa.insert(rotulo.to_owned(), Segredo::novo(texto));
    }
    (pos == bytes.len()).then_some(mapa)
}

#[derive(Deserialize)]
struct ConteudoV1 {
    entries: Vec<EntradaV1>,
}

#[derive(Deserialize)]
struct EntradaV1 {
    connection: i64,
    provider: String,
    profile: i64,
    #[serde(deserialize_with = "valor_zerado")]
    secret: Zeroizing<String>,
}

fn valor_zerado<'de, D: Deserializer<'de>>(d: D) -> Result<Zeroizing<String>, D::Error> {
    String::deserialize(d).map(Zeroizing::new)
}

fn decodificar_v1(bytes: &[u8]) -> Option<Mapa> {
    let conteudo: ConteudoV1 = serde_json::from_slice(bytes).ok()?;
    let mut mapa = Mapa::new();
    for entrada in conteudo.entries {
        let rotulo = format!(
            "{}/{}/{}",
            entrada.provider, entrada.profile, entrada.connection
        );
        rotulo::validar(&rotulo).ok()?;
        if entrada.secret.is_empty() || mapa.contains_key(&rotulo) {
            return None;
        }
        let EntradaV1 { mut secret, .. } = entrada;
        // Move o `String` (sem copiar o valor); o que fica no `Zeroizing` é vazio.
        mapa.insert(rotulo, Segredo::novo(std::mem::take(&mut *secret)));
    }
    Some(mapa)
}

/// Decodifica o conteúdo decifrado conforme a versão do cabeçalho. Conteúdo que não se
/// entende é [`ErroDeSegredo::SenhaOuArquivoInvalido`], como qualquer arquivo inválido.
pub(crate) fn decodificar(versao: u16, bytes: &[u8]) -> Result<Mapa, ErroDeSegredo> {
    let mapa = match versao {
        VERSAO_1 => decodificar_v1(bytes),
        VERSAO_2 => decodificar_v2(bytes),
        _ => None,
    };
    mapa.ok_or(ErroDeSegredo::SenhaOuArquivoInvalido)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapa(pares: &[(&str, &str)]) -> Mapa {
        pares
            .iter()
            .map(|(r, v)| ((*r).to_owned(), Segredo::novo(*v)))
            .collect()
    }

    fn valores(m: &Mapa) -> Vec<(String, String)> {
        m.iter()
            .map(|(r, v)| (r.clone(), v.expor().to_owned()))
            .collect()
    }

    #[test]
    fn formato_2_ida_e_volta_com_capacidade_exata() {
        let original = mapa(&[("cpf", "000.000.000-00"), ("senha:ç", "v\u{0}a\"lor")]);
        let bytes = codificar(&original).unwrap();
        assert_eq!(bytes.len(), bytes.capacity());
        let lido = decodificar(VERSAO_2, &bytes).unwrap();
        assert_eq!(valores(&lido), valores(&original));
        assert_eq!(
            valores(&decodificar(VERSAO_2, &codificar(&Mapa::new()).unwrap()).unwrap()),
            vec![]
        );
    }

    #[test]
    fn formato_2_malformado_e_recusado_sem_panico() {
        let bytes = codificar(&mapa(&[("cpf", "valor"), ("outro", "valor2")])).unwrap();
        for n in 0..bytes.len() {
            assert!(
                decodificar(VERSAO_2, &bytes[..n]).is_err(),
                "cortado em {n}"
            );
        }
        let mut sobra = bytes.to_vec();
        sobra.push(0);
        assert!(decodificar(VERSAO_2, &sobra).is_err());
        // Quantidade absurda não aloca nada antes de falhar.
        assert!(decodificar(VERSAO_2, &u32::MAX.to_le_bytes()).is_err());
        // Rótulo repetido.
        let mut repetido = 2u32.to_le_bytes().to_vec();
        for _ in 0..2 {
            repetido.extend_from_slice(&3u32.to_le_bytes());
            repetido.extend_from_slice(b"cpf");
            repetido.extend_from_slice(&1u32.to_le_bytes());
            repetido.extend_from_slice(b"x");
        }
        assert!(decodificar(VERSAO_2, &repetido).is_err());
    }

    #[test]
    fn formato_1_vira_rotulos_de_texto() {
        let json =
            br#"{"entries":[{"connection":7,"provider":"lichess","profile":1,"secret":"lip_x"}]}"#;
        let lido = decodificar(VERSAO_1, json).unwrap();
        assert_eq!(
            valores(&lido),
            vec![("lichess/1/7".to_owned(), "lip_x".to_owned())]
        );
        assert!(decodificar(VERSAO_1, b"{nao e json").is_err());
        assert!(decodificar(9, json).is_err());
    }
}
