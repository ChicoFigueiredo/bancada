//! Validação do rótulo de um segredo. O rótulo não é secreto (vai para o índice em claro e é a
//! conta no keyring), mas também não pode carregar caractere de controle nem crescer sem fim.

use crate::erro::ErroDeSegredo;

/// Tamanho máximo de um rótulo, em bytes UTF-8.
pub const TAMANHO_MAXIMO_DO_ROTULO: usize = 256;

pub(crate) fn validar(rotulo: &str) -> Result<(), ErroDeSegredo> {
    if rotulo.is_empty()
        || rotulo.len() > TAMANHO_MAXIMO_DO_ROTULO
        || rotulo.chars().any(char::is_control)
    {
        return Err(ErroDeSegredo::RotuloInvalido);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aceita_rotulos_comuns_e_recusa_os_estranhos() {
        for bom in [
            "cpf",
            "senha:nubank",
            "lichess/1/7",
            "ção e espaço",
            &"a".repeat(256),
        ] {
            assert_eq!(validar(bom), Ok(()), "{bom}");
        }
        for ruim in ["", "a\nb", "a\tb", "\u{7f}", "\u{85}", &"a".repeat(257)] {
            assert_eq!(
                validar(ruim),
                Err(ErroDeSegredo::RotuloInvalido),
                "{ruim:?}"
            );
        }
    }
}
