//! Limites por host: balde de fichas (na forma GCRA, só com inteiros), `Retry-After`
//! (segundos ou data HTTP) e backoff exponencial com jitter.

use crate::relogio::{Instante, milissegundos};
use std::time::Duration;

/// Pausa quando o `429` não diz quanto esperar.
pub(crate) const PAUSA_PADRAO: Duration = Duration::from_secs(60);

/// Ritmo permitido para um host: rajada de `rajada`, depois um pedido a cada `a_cada`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ritmo {
    pub rajada: u32,
    pub a_cada: Duration,
}

impl Default for Ritmo {
    /// Rajada de 10, depois 5 por segundo: folgado para um app de um usuário só, e ainda um
    /// teto contra um laço que dispare pedidos sem parar.
    fn default() -> Self {
        Self {
            rajada: 10,
            a_cada: Duration::from_millis(200),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Balde {
    intervalo_ms: i64,
    tolerancia_ms: i64,
    /// Instante teórico da próxima chegada (GCRA).
    proxima: Option<i64>,
}

impl Balde {
    pub fn novo(ritmo: Ritmo) -> Self {
        let intervalo_ms = milissegundos(ritmo.a_cada).max(1);
        let rajada = i64::from(ritmo.rajada.max(1));
        Self {
            intervalo_ms,
            tolerancia_ms: intervalo_ms.saturating_mul(rajada - 1),
            proxima: None,
        }
    }

    /// Reserva uma vaga e devolve o instante em que o pedido pode sair (`agora` se já pode).
    pub fn reservar(&mut self, agora: Instante) -> Instante {
        let agora = agora.em_milissegundos();
        let proxima = self.proxima.map_or(agora, |t| t.max(agora));
        let liberado = (proxima - self.tolerancia_ms).max(agora);
        self.proxima = Some(proxima + self.intervalo_ms);
        Instante::de_milissegundos(liberado)
    }
}

/// Quantas vezes e com que espera repetir falhas temporárias (5xx, conexão interrompida). Só
/// para `GET`; erros do cliente (4xx) nunca se repetem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoliticaDeRepeticao {
    pub repeticoes: u32,
    pub espera_base: Duration,
    pub espera_maxima: Duration,
}

impl Default for PoliticaDeRepeticao {
    fn default() -> Self {
        Self {
            repeticoes: 2,
            espera_base: Duration::from_millis(500),
            espera_maxima: Duration::from_secs(30),
        }
    }
}

impl PoliticaDeRepeticao {
    /// Sem repetição.
    pub fn nenhuma() -> Self {
        Self {
            repeticoes: 0,
            ..Self::default()
        }
    }

    /// Espera antes da repetição `tentativa` (0 = primeira): exponencial com "equal jitter",
    /// entre metade e o total de `base * 2^tentativa` (limitado a `espera_maxima`).
    pub(crate) fn espera(&self, tentativa: u32, sorteio: &mut SplitMix64) -> Duration {
        let fator = 1u32.checked_shl(tentativa.min(20)).unwrap_or(u32::MAX);
        let total = self
            .espera_base
            .saturating_mul(fator)
            .min(self.espera_maxima);
        let total_ms = u64::try_from(total.as_millis()).unwrap_or(u64::MAX);
        let metade = total_ms / 2;
        let jitter = if total_ms - metade == 0 {
            0
        } else {
            sorteio.proximo() % (total_ms - metade + 1)
        };
        Duration::from_millis(metade + jitter)
    }
}

/// Gerador pequeno e determinístico (semente na configuração; testes reprodutíveis).
#[derive(Debug, Clone)]
pub(crate) struct SplitMix64(u64);

impl SplitMix64 {
    pub fn novo(semente: u64) -> Self {
        Self(semente)
    }

    pub fn proximo(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// `Retry-After` em segundos (`120`) ou data HTTP (`Wed, 11 Nov 2026 12:02:00 GMT`).
pub(crate) fn tentar_de_novo_em(valor: &str, agora: Instante) -> Option<Instante> {
    let valor = valor.trim();
    if let Ok(segundos) = valor.parse::<u64>() {
        return Some(agora.mais(Duration::from_secs(segundos.min(24 * 3600))));
    }
    data_http(valor).map(|em| em.max(agora))
}

/// IMF-fixdate (RFC 9110): `Sun, 06 Nov 1994 08:49:37 GMT`.
fn data_http(valor: &str) -> Option<Instante> {
    let (_, resto) = valor.split_once(", ")?;
    let mut partes = resto.split(' ');
    let dia: i64 = partes.next()?.parse().ok()?;
    let mes = match partes.next()? {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    };
    let ano: i64 = partes.next()?.parse().ok()?;
    let mut hms = partes.next()?.split(':');
    let h: i64 = hms.next()?.parse().ok()?;
    let m: i64 = hms.next()?.parse().ok()?;
    let s: i64 = hms.next()?.parse().ok()?;
    if partes.next()? != "GMT" || !(1..=31).contains(&dia) || h > 23 || m > 59 || s > 60 {
        return None;
    }
    let dias = dias_desde_1970(ano, mes, dia);
    Some(Instante::de_milissegundos(
        ((dias * 24 + h) * 60 + m) * 60_000 + s * 1000,
    ))
}

/// Dias desde 1970-01-01 (algoritmo de Howard Hinnant).
fn dias_desde_1970(ano: i64, mes: i64, dia: i64) -> i64 {
    let y = if mes <= 2 { ano - 1 } else { ano };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (mes + 9) % 12;
    let doy = (153 * mp + 2) / 5 + dia - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn datas_http_batem_com_a_epoca() {
        assert_eq!(
            data_http("Thu, 01 Jan 1970 00:00:00 GMT"),
            Some(Instante::de_milissegundos(0))
        );
        assert_eq!(
            data_http("Sun, 06 Nov 1994 08:49:37 GMT"),
            Some(Instante::de_milissegundos(784_111_777_000))
        );
        assert_eq!(data_http("ontem"), None);
    }

    #[test]
    fn data_passada_quer_dizer_agora() {
        let agora = Instante::de_milissegundos(1_000_000_000_000);
        assert_eq!(
            tentar_de_novo_em("Thu, 01 Jan 1970 00:00:00 GMT", agora),
            Some(agora)
        );
        assert_eq!(
            tentar_de_novo_em("120", agora),
            Some(agora.mais(Duration::from_secs(120)))
        );
    }

    #[test]
    fn esperas_com_jitter_ficam_na_janela() {
        let politica = PoliticaDeRepeticao {
            repeticoes: 5,
            espera_base: Duration::from_secs(1),
            espera_maxima: Duration::from_secs(4),
        };
        let mut sorteio = SplitMix64::novo(1);
        for _ in 0..200 {
            let e0 = politica.espera(0, &mut sorteio);
            assert!(e0 >= Duration::from_millis(500) && e0 <= Duration::from_secs(1));
            let e5 = politica.espera(5, &mut sorteio);
            assert!(e5 >= Duration::from_secs(2) && e5 <= Duration::from_secs(4));
        }
    }

    #[test]
    fn o_balde_segura_depois_da_rajada() {
        let mut balde = Balde::novo(Ritmo {
            rajada: 2,
            a_cada: Duration::from_secs(1),
        });
        let t0 = Instante::de_milissegundos(0);
        assert_eq!(balde.reservar(t0), t0);
        assert_eq!(balde.reservar(t0), t0);
        assert_eq!(balde.reservar(t0), Instante::de_milissegundos(1_000));
    }
}
