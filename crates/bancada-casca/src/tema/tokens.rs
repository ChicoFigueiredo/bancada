//! Leitura do formato de tokens do Chess Studio e do SBF: `tokens.json` (papéis de cor com
//! `dark`/`light`, tipografia, espaço, raio, sombra e tamanhos) e `temas.json` (catálogo de
//! Temas, cada um com uma base e as cores que trocam). O app passa o texto dos dois arquivos;
//! a casca não embute os tokens de nenhum app.

use egui::Color32;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::fmt;

/// Base de um Tema: os valores `dark` ou `light` do `tokens.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum Base {
    #[serde(rename = "dark")]
    Escura,
    #[serde(rename = "light")]
    Clara,
}

/// Por que os tokens não puderam ser lidos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroTokens {
    Json(String),
    Ausente(String),
    ValorInvalido(String),
    TemaRepetido(String),
    TemaComTokenDesconhecido { tema: String, token: String },
}

impl fmt::Display for ErroTokens {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErroTokens::Json(e) => write!(f, "JSON de tokens ilegível: {e}"),
            ErroTokens::Ausente(n) => write!(f, "token ausente: {n}"),
            ErroTokens::ValorInvalido(v) => write!(f, "valor de token inválido: {v}"),
            ErroTokens::TemaRepetido(t) => write!(f, "tema repetido: {t}"),
            ErroTokens::TemaComTokenDesconhecido { tema, token } => {
                write!(f, "o tema {tema} troca o token {token}, que não existe")
            }
        }
    }
}

impl std::error::Error for ErroTokens {}

#[derive(Deserialize)]
struct ArquivoTokens {
    color: CoresBrutas,
    #[serde(rename = "type")]
    tipografia: TipografiaBruta,
    spacing: MedidasBrutas,
    radius: MedidasBrutas,
    shadow: SombrasBrutas,
    size: MedidasBrutas,
}

#[derive(Deserialize)]
struct CoresBrutas {
    tokens: Vec<CorBruta>,
}

#[derive(Deserialize)]
struct CorBruta {
    name: String,
    value: ParBruto,
}

#[derive(Deserialize)]
struct ParBruto {
    dark: String,
    light: String,
}

#[derive(Deserialize)]
struct MedidasBrutas {
    tokens: Vec<MedidaBruta>,
}

#[derive(Deserialize)]
struct MedidaBruta {
    name: String,
    value: String,
}

#[derive(Deserialize)]
struct SombrasBrutas {
    tokens: Vec<CorBruta>,
}

#[derive(Deserialize)]
struct TipografiaBruta {
    groups: Vec<GrupoTipoBruto>,
}

#[derive(Deserialize)]
struct GrupoTipoBruto {
    styles: Vec<EstiloBruto>,
}

#[derive(Deserialize)]
struct EstiloBruto {
    name: String,
    #[serde(rename = "fontSize")]
    tamanho: String,
    #[serde(rename = "lineHeight")]
    altura_linha: String,
    #[serde(rename = "fontWeight")]
    peso: u16,
}

/// Estilo tipográfico (`body`, `caption`…), em pixels lógicos.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EstiloTexto {
    pub tamanho: f32,
    pub altura_linha: f32,
    pub peso: u16,
}

/// Os valores do `tokens.json`, pelo nome do token.
#[derive(Debug, Clone, PartialEq)]
pub struct Tokens {
    cores: BTreeMap<String, (Color32, Color32)>,
    medidas: HashMap<String, f32>,
    sombras: HashMap<String, (egui::Shadow, egui::Shadow)>,
    textos: HashMap<String, EstiloTexto>,
}

impl Tokens {
    pub fn ler(json: &str) -> Result<Self, ErroTokens> {
        let bruto: ArquivoTokens =
            serde_json::from_str(json).map_err(|e| ErroTokens::Json(e.to_string()))?;
        let mut cores = BTreeMap::new();
        for c in bruto.color.tokens {
            cores.insert(c.name, (ler_hex(&c.value.dark)?, ler_hex(&c.value.light)?));
        }
        let mut medidas = HashMap::new();
        for m in bruto
            .spacing
            .tokens
            .into_iter()
            .chain(bruto.radius.tokens)
            .chain(bruto.size.tokens)
        {
            medidas.insert(m.name, ler_px(&m.value)?);
        }
        let mut sombras = HashMap::new();
        for s in bruto.shadow.tokens {
            sombras.insert(
                s.name,
                (ler_sombra(&s.value.dark)?, ler_sombra(&s.value.light)?),
            );
        }
        let mut textos = HashMap::new();
        for e in bruto.tipografia.groups.into_iter().flat_map(|g| g.styles) {
            textos.insert(
                e.name,
                EstiloTexto {
                    tamanho: ler_px(&e.tamanho)?,
                    altura_linha: ler_px(&e.altura_linha)?,
                    peso: e.peso,
                },
            );
        }
        Ok(Self {
            cores,
            medidas,
            sombras,
            textos,
        })
    }

    pub fn cor(&self, nome: &str, base: Base) -> Option<Color32> {
        self.cores.get(nome).map(|(escura, clara)| match base {
            Base::Escura => *escura,
            Base::Clara => *clara,
        })
    }

    pub fn nomes_de_cor(&self) -> impl Iterator<Item = &str> {
        self.cores.keys().map(String::as_str)
    }

    /// Espaço, raio ou tamanho (`space-2`, `radius-m`, `control-h`…).
    pub fn medida(&self, nome: &str) -> Option<f32> {
        self.medidas.get(nome).copied()
    }

    pub fn sombra(&self, nome: &str, base: Base) -> Option<egui::Shadow> {
        self.sombras.get(nome).map(|(escura, clara)| match base {
            Base::Escura => *escura,
            Base::Clara => *clara,
        })
    }

    pub fn texto(&self, nome: &str) -> Option<EstiloTexto> {
        self.textos.get(nome).copied()
    }
}

/// Um Tema do catálogo, ainda sem resolver.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DescricaoTema {
    pub id: String,
    #[serde(rename = "name")]
    pub nome: String,
    pub base: Base,
    #[serde(rename = "family", default)]
    pub familia: String,
    #[serde(rename = "inspiration", default)]
    pub inspiracao: String,
    #[serde(rename = "colors", default)]
    cores: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct ArquivoTemas {
    themes: Vec<DescricaoTema>,
}

/// Um Tema resolvido: a base com as cores do Tema por cima.
#[derive(Debug, Clone, PartialEq)]
pub struct TemaResolvido {
    pub id: String,
    pub nome: String,
    pub base: Base,
    cores: BTreeMap<String, Color32>,
}

impl TemaResolvido {
    /// Cor pelo nome do token (`"success"`, `"board-light"`…).
    pub fn cor(&self, nome: &str) -> Option<Color32> {
        self.cores.get(nome).copied()
    }
}

/// `tokens.json` + `temas.json`: os Temas que o usuário pode escolher.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogoDeTemas {
    tokens: Tokens,
    temas: Vec<TemaResolvido>,
    descricoes: Vec<DescricaoTema>,
}

impl CatalogoDeTemas {
    pub fn ler(tokens_json: &str, temas_json: &str) -> Result<Self, ErroTokens> {
        let tokens = Tokens::ler(tokens_json)?;
        let arquivo: ArquivoTemas =
            serde_json::from_str(temas_json).map_err(|e| ErroTokens::Json(e.to_string()))?;
        let mut temas: Vec<TemaResolvido> = Vec::new();
        for d in &arquivo.themes {
            if temas.iter().any(|t| t.id == d.id) {
                return Err(ErroTokens::TemaRepetido(d.id.clone()));
            }
            let mut cores: BTreeMap<String, Color32> = tokens
                .nomes_de_cor()
                .filter_map(|n| Some((n.to_owned(), tokens.cor(n, d.base)?)))
                .collect();
            for (nome, valor) in &d.cores {
                if !cores.contains_key(nome) {
                    return Err(ErroTokens::TemaComTokenDesconhecido {
                        tema: d.id.clone(),
                        token: nome.clone(),
                    });
                }
                cores.insert(nome.clone(), ler_hex(valor)?);
            }
            temas.push(TemaResolvido {
                id: d.id.clone(),
                nome: d.nome.clone(),
                base: d.base,
                cores,
            });
        }
        Ok(Self {
            tokens,
            temas,
            descricoes: arquivo.themes,
        })
    }

    pub fn tokens(&self) -> &Tokens {
        &self.tokens
    }

    pub fn temas(&self) -> &[TemaResolvido] {
        &self.temas
    }

    pub fn descricoes(&self) -> &[DescricaoTema] {
        &self.descricoes
    }

    pub fn tema(&self, id: &str) -> Option<&TemaResolvido> {
        self.temas.iter().find(|t| t.id == id)
    }
}

/// `#rrggbb` ou `#rrggbbaa` (alfa não pré-multiplicado no JSON).
pub fn ler_hex(texto: &str) -> Result<Color32, ErroTokens> {
    let ruim = || ErroTokens::ValorInvalido(texto.to_owned());
    let digitos = texto.strip_prefix('#').ok_or_else(ruim)?;
    let byte = |i: usize| {
        digitos
            .get(i..i + 2)
            .and_then(|h| u8::from_str_radix(h, 16).ok())
            .ok_or_else(ruim)
    };
    match digitos.len() {
        6 => Ok(Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?)),
        8 => Ok(Color32::from_rgba_unmultiplied(
            byte(0)?,
            byte(2)?,
            byte(4)?,
            byte(6)?,
        )),
        _ => Err(ruim()),
    }
}

/// `"32px"` → 32.0.
pub fn ler_px(texto: &str) -> Result<f32, ErroTokens> {
    texto
        .trim()
        .strip_suffix("px")
        .and_then(|n| n.trim().parse::<f32>().ok())
        .ok_or_else(|| ErroTokens::ValorInvalido(texto.to_owned()))
}

/// `"0 4px 12px rgba(0,0,0,0.45)"` (x, y, desfoque, cor; sem espalhamento).
pub fn ler_sombra(texto: &str) -> Result<egui::Shadow, ErroTokens> {
    let ruim = || ErroTokens::ValorInvalido(texto.to_owned());
    let (medidas, cor) = texto.split_once("rgba(").ok_or_else(ruim)?;
    let comprimento = |t: &str| -> Option<f32> {
        if t == "0" {
            return Some(0.0);
        }
        t.strip_suffix("px")?.parse().ok()
    };
    let mut partes = medidas.split_whitespace();
    let mut proxima = || comprimento(partes.next()?);
    let x = proxima().ok_or_else(ruim)?;
    let y = proxima().ok_or_else(ruim)?;
    let desfoque = proxima().ok_or_else(ruim)?;
    let canais: Vec<&str> = cor
        .trim_end_matches(')')
        .split(',')
        .map(str::trim)
        .collect();
    let [r, g, b, a] = canais[..] else {
        return Err(ruim());
    };
    let canal = |s: &str| s.parse::<u8>().map_err(|_| ruim());
    let alfa: f32 = a.parse().map_err(|_| ruim())?;
    Ok(egui::Shadow {
        offset: [x as i8, y as i8],
        blur: desfoque.clamp(0.0, 255.0) as u8,
        spread: 0,
        color: Color32::from_rgba_unmultiplied(
            canal(r)?,
            canal(g)?,
            canal(b)?,
            (alfa.clamp(0.0, 1.0) * 255.0).round() as u8,
        ),
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_hex_px_e_sombra() {
        assert_eq!(ler_hex("#ff0000").unwrap(), Color32::from_rgb(255, 0, 0));
        assert_eq!(
            ler_hex("#00000080").unwrap(),
            Color32::from_rgba_unmultiplied(0, 0, 0, 128)
        );
        assert!(ler_hex("ff0000").is_err());
        assert_eq!(ler_px("32px").unwrap(), 32.0);
        let s = ler_sombra("0 4px 12px rgba(24,35,31,0.16)").unwrap();
        assert_eq!(s.offset, [0, 4]);
        assert_eq!(s.blur, 12);
    }
}
