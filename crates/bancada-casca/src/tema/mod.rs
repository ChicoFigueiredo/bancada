//! Temas por tokens JSON, Densidade e Escala. O Tema troca o `Visuals` inteiro; a Densidade
//! muda o `Spacing`; a Escala é o `zoom_factor` do egui. O app consulta os tokens semânticos a
//! partir do `egui::Context` com [`TemaAtivo::do_contexto`].

pub mod tokens;

use egui::{Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, Style, TextStyle, vec2};
use std::sync::Arc;
pub use tokens::{
    Base, CatalogoDeTemas, DescricaoTema, ErroTokens, EstiloTexto, TemaResolvido, Tokens,
};

/// Densidade da interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum Densidade {
    #[default]
    Confortavel,
    /// Um passo abaixo nos respiros (grade de 4 px) e controles de 24 px.
    Compacta,
}

/// Escalas oferecidas nas Opções (multiplicam todas as medidas).
pub const ESCALAS: [f32; 6] = [0.9, 1.0, 1.1, 1.25, 1.5, 1.75];

/// Os papéis de cor semânticos, com nomes em português. Os tokens que um app não tem (o Chess
/// não tem `gold`, nem `hero-*`) caem num papel próximo, documentado em cada campo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paleta {
    /// `surface-0`: fundo da janela.
    pub superficie_0: Color32,
    /// `surface-1`: painéis, dock, diálogos.
    pub superficie_1: Color32,
    /// `surface-2`: Ribbon, cabeçalhos, campos, barra de status.
    pub superficie_2: Color32,
    /// `surface-3`: hover.
    pub superficie_3: Color32,
    /// `surface-4`: pressionado.
    pub superficie_4: Color32,
    /// `border`: divisórias (decorativo).
    pub borda: Color32,
    /// `border-strong`: contorno de controles.
    pub borda_forte: Color32,
    /// `ink`: texto principal.
    pub tinta: Color32,
    /// `ink-muted`: texto secundário (o `--text-3` do SBF).
    pub tinta_suave: Color32,
    /// `ink-disabled`.
    pub tinta_desabilitada: Color32,
    /// `accent`: cor de ação.
    pub destaque: Color32,
    pub destaque_hover: Color32,
    pub destaque_pressionado: Color32,
    /// `on-accent`: texto sobre `accent`.
    pub sobre_destaque: Color32,
    /// `accent-subtle`: seleção, estado ligado.
    pub destaque_sutil: Color32,
    /// `accent-border` (cai em `accent-ink`).
    pub destaque_borda: Color32,
    /// `accent-ink`: texto verde, links.
    pub destaque_tinta: Color32,
    pub anel_de_foco: Color32,
    pub info: Color32,
    /// `info-subtle` (cai em `surface-3`).
    pub info_sutil: Color32,
    pub sucesso: Color32,
    /// `success-subtle` (cai em `accent-subtle`).
    pub sucesso_sutil: Color32,
    pub aviso: Color32,
    /// `warning-subtle` (cai em `surface-3`).
    pub aviso_sutil: Color32,
    pub perigo: Color32,
    /// `danger-subtle` (cai em `surface-3`).
    pub perigo_sutil: Color32,
    /// `gold` (cai em `warning`).
    pub ouro: Color32,
    /// `gold-subtle` (cai em `surface-3`).
    pub ouro_sutil: Color32,
    /// `scrim`: véu de modal.
    pub veu: Color32,
}

/// Tokens de cor que todo app precisa ter (o mínimo comum entre o Chess e o SBF).
pub const TOKENS_OBRIGATORIOS: [&str; 22] = [
    "surface-0",
    "surface-1",
    "surface-2",
    "surface-3",
    "surface-4",
    "border",
    "border-strong",
    "ink",
    "ink-muted",
    "ink-disabled",
    "accent",
    "accent-hover",
    "accent-pressed",
    "on-accent",
    "accent-subtle",
    "accent-ink",
    "focus-ring",
    "info",
    "success",
    "warning",
    "danger",
    "scrim",
];

impl Paleta {
    pub fn de(tema: &TemaResolvido) -> Result<Self, ErroTokens> {
        let c = |n: &str| tema.cor(n).ok_or_else(|| ErroTokens::Ausente(n.to_owned()));
        for n in TOKENS_OBRIGATORIOS {
            c(n)?;
        }
        let ou = |n: &str, reserva: &str| tema.cor(n).or_else(|| tema.cor(reserva));
        let o = |n: &str, reserva: &str| ou(n, reserva).unwrap_or(Color32::MAGENTA);
        Ok(Self {
            superficie_0: c("surface-0")?,
            superficie_1: c("surface-1")?,
            superficie_2: c("surface-2")?,
            superficie_3: c("surface-3")?,
            superficie_4: c("surface-4")?,
            borda: c("border")?,
            borda_forte: c("border-strong")?,
            tinta: c("ink")?,
            tinta_suave: c("ink-muted")?,
            tinta_desabilitada: c("ink-disabled")?,
            destaque: c("accent")?,
            destaque_hover: c("accent-hover")?,
            destaque_pressionado: c("accent-pressed")?,
            sobre_destaque: c("on-accent")?,
            destaque_sutil: c("accent-subtle")?,
            destaque_borda: o("accent-border", "accent-ink"),
            destaque_tinta: c("accent-ink")?,
            anel_de_foco: c("focus-ring")?,
            info: c("info")?,
            info_sutil: o("info-subtle", "surface-3"),
            sucesso: c("success")?,
            sucesso_sutil: o("success-subtle", "accent-subtle"),
            aviso: c("warning")?,
            aviso_sutil: o("warning-subtle", "surface-3"),
            perigo: c("danger")?,
            perigo_sutil: o("danger-subtle", "surface-3"),
            ouro: o("gold", "warning"),
            ouro_sutil: o("gold-subtle", "surface-3"),
            veu: c("scrim")?,
        })
    }
}

/// O Tema em uso, com a Densidade e a Escala, guardado no `egui::Context` para o app consultar.
#[derive(Debug, Clone, PartialEq)]
pub struct TemaAtivo {
    pub tema: TemaResolvido,
    pub paleta: Paleta,
    pub densidade: Densidade,
    pub escala: f32,
    pub medidas: Medidas,
    tokens: Arc<Tokens>,
}

/// Medidas que dependem da Densidade (pixels lógicos na Escala 100%).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Medidas {
    pub altura_controle: f32,
    pub respiro_painel: f32,
    pub raio_s: f32,
    pub raio_m: f32,
    pub raio_l: f32,
    pub raio_xl: f32,
    pub icone_p: f32,
    pub icone_g: f32,
    pub altura_abas_ribbon: f32,
    pub altura_ribbon_classico: f32,
    pub altura_ribbon_simples: f32,
    pub altura_status: f32,
    pub altura_cabecalho_painel: f32,
}

impl Medidas {
    pub fn de(tokens: &Tokens, densidade: Densidade) -> Self {
        let m = |n: &str, reserva: f32| tokens.medida(n).unwrap_or(reserva);
        let compacta = densidade == Densidade::Compacta;
        Self {
            altura_controle: if compacta {
                m("control-h-compact", 24.0)
            } else {
                m("control-h", 32.0)
            },
            respiro_painel: if compacta {
                m("space-3", 12.0)
            } else {
                m("space-4", 16.0)
            },
            raio_s: m("radius-s", 2.0),
            raio_m: m("radius-m", 4.0),
            raio_l: m("radius-l", 6.0),
            raio_xl: m("radius-xl", 8.0),
            icone_p: m("icon-s", 16.0),
            icone_g: m("icon-l", 32.0),
            altura_abas_ribbon: if compacta {
                m("control-h-compact", 24.0) + 4.0
            } else {
                m("ribbon-tabs-h", 32.0)
            },
            altura_ribbon_classico: if compacta {
                m("ribbon-classic-h", 96.0) - 12.0
            } else {
                m("ribbon-classic-h", 96.0)
            },
            altura_ribbon_simples: if compacta {
                m("ribbon-simple-h", 40.0) - 8.0
            } else {
                m("ribbon-simple-h", 40.0)
            },
            altura_status: m("statusbar-h", 24.0),
            altura_cabecalho_painel: if compacta {
                m("control-h-compact", 24.0) + 4.0
            } else {
                m("panel-header-h", 32.0)
            },
        }
    }
}

fn id_no_contexto() -> egui::Id {
    egui::Id::new("bancada-casca.tema-ativo")
}

impl TemaAtivo {
    pub fn novo(
        catalogo: &CatalogoDeTemas,
        tema_id: &str,
        densidade: Densidade,
        escala: f32,
    ) -> Result<Self, ErroTokens> {
        let tema = catalogo
            .tema(tema_id)
            .or_else(|| catalogo.temas().first())
            .ok_or_else(|| ErroTokens::Ausente(format!("tema {tema_id}")))?
            .clone();
        let paleta = Paleta::de(&tema)?;
        Ok(Self {
            paleta,
            densidade,
            escala,
            medidas: Medidas::de(catalogo.tokens(), densidade),
            tokens: Arc::new(catalogo.tokens().clone()),
            tema,
        })
    }

    pub fn tokens(&self) -> &Tokens {
        &self.tokens
    }

    /// Cor de qualquer token pelo nome, já com o Tema aplicado.
    pub fn cor(&self, nome: &str) -> Option<Color32> {
        self.tema.cor(nome)
    }

    /// O Tema em uso neste `Context` (a casca grava a cada troca). `None` antes da casca rodar.
    pub fn do_contexto(ctx: &egui::Context) -> Option<Arc<TemaAtivo>> {
        ctx.data(|d| d.get_temp::<Arc<TemaAtivo>>(id_no_contexto()))
    }

    /// Paleta do `Context`, ou a de reserva do egui (para widgets desenhados fora da casca).
    pub fn paleta_do_contexto(ctx: &egui::Context) -> Option<Paleta> {
        Self::do_contexto(ctx).map(|t| t.paleta)
    }

    /// Aplica Visuals, Spacing e Escala ao `Context` e grava o Tema para consulta.
    pub fn aplicar(self: &Arc<Self>, ctx: &egui::Context) {
        ctx.set_style_of(egui::Theme::Dark, montar_estilo(self));
        ctx.set_style_of(egui::Theme::Light, montar_estilo(self));
        ctx.set_theme(match self.tema.base {
            Base::Escura => egui::Theme::Dark,
            Base::Clara => egui::Theme::Light,
        });
        if (ctx.zoom_factor() - self.escala).abs() > f32::EPSILON {
            ctx.set_zoom_factor(self.escala);
        }
        ctx.data_mut(|d| d.insert_temp(id_no_contexto(), Arc::clone(self)));
    }
}

fn raio(px: f32) -> CornerRadius {
    CornerRadius::same(px.round().clamp(0.0, 255.0) as u8)
}

/// `Visuals` de um Tema, preenchido só com tokens (porte do `style.rs` do Chess Studio).
pub fn montar_visuals(tema: &TemaAtivo) -> egui::Visuals {
    let p = &tema.paleta;
    let base = tema.tema.base;
    let mut v = match base {
        Base::Escura => egui::Visuals::dark(),
        Base::Clara => egui::Visuals::light(),
    };
    let tokens = tema.tokens();
    let sombra = |n: &str| tokens.sombra(n, base).unwrap_or_default();
    let fio = |c: Color32| Stroke::new(1.0, c);
    let m = &tema.medidas;

    v.override_text_color = None;
    v.weak_text_color = Some(p.tinta_suave);
    v.hyperlink_color = p.destaque_tinta;
    v.faint_bg_color = p.superficie_2;
    v.extreme_bg_color = p.superficie_2;
    v.text_edit_bg_color = None;
    v.code_bg_color = p.superficie_2;
    v.warn_fg_color = p.aviso;
    v.error_fg_color = p.perigo;
    v.panel_fill = p.superficie_1;
    v.window_fill = p.superficie_1;
    v.window_stroke = fio(p.borda);
    v.window_corner_radius = raio(m.raio_xl);
    v.window_shadow = sombra("shadow-floating");
    v.menu_corner_radius = raio(m.raio_l);
    v.popup_shadow = sombra("shadow-flyout");
    v.selection.bg_fill = p.destaque_sutil;
    v.selection.stroke = fio(p.destaque_tinta);
    v.text_cursor.stroke = Stroke::new(2.0, p.tinta);
    v.striped = false;

    let canto = raio(m.raio_m);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.superficie_1;
    w.noninteractive.weak_bg_fill = p.superficie_1;
    w.noninteractive.bg_stroke = fio(p.borda);
    w.noninteractive.fg_stroke = fio(p.tinta);
    w.noninteractive.corner_radius = canto;
    for (estado, fundo) in [
        (&mut w.inactive, p.superficie_2),
        (&mut w.hovered, p.superficie_3),
        (&mut w.active, p.superficie_4),
        (&mut w.open, p.superficie_3),
    ] {
        estado.bg_fill = fundo;
        estado.weak_bg_fill = fundo;
        estado.bg_stroke = fio(p.borda_forte);
        estado.fg_stroke = fio(p.tinta);
        estado.corner_radius = canto;
        estado.expansion = 0.0;
    }
    // Botões sutis (os do Ribbon, os itens de navegação) não têm contorno em repouso.
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.weak_bg_fill = Color32::TRANSPARENT;
    v.disabled_alpha = 0.5;
    v
}

/// Nome do estilo de texto `ribbon-label` (12/14).
pub const ROTULO_RIBBON: &str = "ribbon-label";
/// Nome do estilo de texto `body-large` (16/22, busca da Paleta).
pub const CORPO_GRANDE: &str = "body-large";

/// `Style` completo: Visuals do Tema, escala tipográfica e Spacing da Densidade.
pub fn montar_estilo(tema: &TemaAtivo) -> Style {
    let tokens = tema.tokens();
    let tam = |n: &str, reserva: f32| tokens.texto(n).map_or(reserva, |t| t.tamanho);
    let sans = |s| FontId::new(s, FontFamily::Proportional);
    let mut estilo = Style {
        visuals: montar_visuals(tema),
        ..Style::default()
    };
    estilo.text_styles = [
        (TextStyle::Small, sans(tam("caption", 12.0))),
        (TextStyle::Body, sans(tam("body", 14.0))),
        (TextStyle::Button, sans(tam("body", 14.0))),
        (TextStyle::Heading, sans(tam("title", 20.0))),
        (
            TextStyle::Name(ROTULO_RIBBON.into()),
            sans(tam(ROTULO_RIBBON, 12.0)),
        ),
        (
            TextStyle::Name(CORPO_GRANDE.into()),
            sans(tam(CORPO_GRANDE, 16.0)),
        ),
        (
            TextStyle::Monospace,
            FontId::new(tam("log", 12.0), FontFamily::Monospace),
        ),
    ]
    .into_iter()
    .collect();
    let m = &tema.medidas;
    let e = |n: &str, reserva: f32| tokens.medida(n).unwrap_or(reserva);
    let sp = &mut estilo.spacing;
    match tema.densidade {
        Densidade::Confortavel => {
            sp.item_spacing = vec2(e("space-2", 8.0), e("space-2", 8.0) - 2.0);
            sp.button_padding = vec2(e("space-2", 8.0), e("space-1", 4.0) + e("space-05", 2.0));
        }
        Densidade::Compacta => {
            sp.item_spacing = vec2(e("space-1", 4.0), e("space-1", 4.0) - 1.0);
            sp.button_padding = vec2(e("space-2", 8.0) - 2.0, e("space-05", 2.0));
        }
    }
    sp.interact_size = vec2(m.altura_controle + 8.0, m.altura_controle);
    sp.window_margin = Margin::same(m.respiro_painel.round() as i8);
    sp.menu_margin = Margin::same((m.respiro_painel / 4.0).round() as i8);
    sp.icon_width = m.icone_p;
    sp.icon_width_inner = m.icone_p - 6.0;
    sp.tooltip_width = 320.0;
    estilo.interaction.tooltip_delay = 0.5;
    estilo.animation_time = 0.12;
    estilo
}

#[cfg(test)]
mod testes {
    use super::*;

    const TOKENS_MINIMOS: &str = r##"{
      "color": {"tokens": [
        {"name":"surface-0","value":{"dark":"#000000","light":"#ffffff"}},
        {"name":"ink","value":{"dark":"#ffffff","light":"#000000"}}
      ]},
      "type": {"groups": []},
      "spacing": {"tokens": []}, "radius": {"tokens": []},
      "shadow": {"tokens": []}, "size": {"tokens": []}
    }"##;

    #[test]
    fn tema_sem_token_obrigatorio_e_recusado() {
        let temas = r#"{"themes":[{"id":"x","name":"X","base":"dark","colors":{}}]}"#;
        let cat = CatalogoDeTemas::ler(TOKENS_MINIMOS, temas).unwrap();
        assert!(matches!(
            TemaAtivo::novo(&cat, "x", Densidade::Confortavel, 1.0),
            Err(ErroTokens::Ausente(_))
        ));
    }

    #[test]
    fn tema_que_troca_token_inexistente_e_recusado() {
        let temas =
            r##"{"themes":[{"id":"x","name":"X","base":"dark","colors":{"nada":"#000000"}}]}"##;
        assert!(matches!(
            CatalogoDeTemas::ler(TOKENS_MINIMOS, temas),
            Err(ErroTokens::TemaComTokenDesconhecido { .. })
        ));
    }

    #[test]
    fn tema_troca_so_as_cores_listadas() {
        let temas =
            r##"{"themes":[{"id":"x","name":"X","base":"light","colors":{"ink":"#123456"}}]}"##;
        let cat = CatalogoDeTemas::ler(TOKENS_MINIMOS, temas).unwrap();
        let t = cat.tema("x").unwrap();
        assert_eq!(t.cor("ink"), Some(Color32::from_rgb(0x12, 0x34, 0x56)));
        assert_eq!(t.cor("surface-0"), Some(Color32::WHITE));
    }
}
