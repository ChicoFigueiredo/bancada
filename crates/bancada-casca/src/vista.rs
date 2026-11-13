//! O desenho da casca em egui: barra de título com a Barra de Acesso Rápido, Ribbon (três
//! modos), bastidores, painel de navegação, trilha, centro, dock, barra de status, painéis
//! flutuantes (e destacados), Paleta, Mapa de atalhos e KeyTips. A lógica fica em
//! [`crate::casca`] e nos modelos puros; aqui só se desenha e se enfileiram pedidos.

use crate::acoes::{Acao, TipoAcao};
use crate::app::{AppCasca, Tom};
use crate::casca::{Casca, Quadro, catalogo_do_contexto};
use crate::icones::FamiliaDeIcones;
use crate::navegacao::{self, Local};
use crate::paleta::{self, Alvo};
use crate::pedidos::{self, Pedido};
use crate::ribbon::{self, Guia, KEYTIP_ARQUIVO, ModoRibbon, NivelKeyTip};
use crate::tema::{Densidade, ESCALAS, Medidas, Paleta, TemaAtivo};
use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Frame, Id, Layout, Margin, Rect, Response,
    RichText, Sense, Stroke, Ui, pos2, vec2,
};
use std::collections::HashMap;

/// Tamanho de um botão de Ação.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TamanhoBotao {
    /// Ícone grande em cima, rótulo embaixo (primeira Ação do grupo no modo clássico).
    Grande,
    /// Ícone pequeno e rótulo ao lado.
    Pequeno,
    /// Só o ícone (Barra de Acesso Rápido).
    Icone,
}

/// Glifo do app para o nome, ou a reserva das fontes padrão do egui.
fn glifo<'a>(icones: &'a FamiliaDeIcones, nome: &str, reserva: &'a str) -> (&'a str, bool) {
    match icones.get(nome) {
        Some(g) => (g, true),
        None => (reserva, false),
    }
}

/// Um botão de Ação da lista única, com dica (motivo se desabilitada) e estado de alternância.
/// Devolve a resposta; o clique numa Ação habilitada já fica enfileirado como pedido.
pub fn botao_acao(
    ui: &mut Ui,
    acao: &Acao,
    tamanho: TamanhoBotao,
    icones: &FamiliaDeIcones,
    paleta: &Paleta,
    medidas: &Medidas,
) -> Response {
    let habilitada = acao.esta_habilitada();
    let cor_texto = if habilitada {
        paleta.tinta
    } else {
        paleta.tinta_desabilitada
    };
    let corpo = ui
        .style()
        .text_styles
        .get(&egui::TextStyle::Body)
        .map_or(14.0, |f| f.size);
    let pequeno = ui
        .style()
        .text_styles
        .get(&egui::TextStyle::Small)
        .map_or(12.0, |f| f.size);
    let fonte_icone = |t: f32| FontId::new(t, icones.familia());
    let painter = ui.painter().clone();
    let icone = icones.get(&acao.icone);
    let alt_linha = (medidas.altura_controle * 0.75).max(22.0);
    let (rect, resposta) = match tamanho {
        TamanhoBotao::Grande => {
            let rotulo = painter.layout(
                acao.rotulo.clone(),
                FontId::proportional(pequeno),
                cor_texto,
                76.0,
            );
            let gi = icone.map(|g| {
                painter.layout_no_wrap(g.to_owned(), fonte_icone(medidas.icone_g), cor_texto)
            });
            let largura = rotulo
                .size()
                .x
                .max(gi.as_ref().map_or(0.0, |g| g.size().x))
                .max(44.0)
                + 12.0;
            let altura = (alt_linha * 3.0).max(64.0);
            let (rect, resp) = ui.allocate_exact_size(vec2(largura, altura), sentido(habilitada));
            pintar_fundo(ui, &resp, rect, acao, paleta, medidas);
            let mut y = rect.top() + 6.0;
            if let Some(g) = gi {
                let pos = pos2(rect.center().x - g.size().x / 2.0, y);
                y += g.size().y + 2.0;
                painter.galley(pos, g, cor_texto);
            } else {
                y = rect.center().y - rotulo.size().y / 2.0;
            }
            painter.galley(
                pos2(rect.center().x - rotulo.size().x / 2.0, y),
                rotulo,
                cor_texto,
            );
            (rect, resp)
        }
        TamanhoBotao::Pequeno => {
            let rotulo = painter.layout_no_wrap(
                acao.rotulo.clone(),
                FontId::proportional(corpo - 1.0),
                cor_texto,
            );
            let gi = icone.map(|g| {
                painter.layout_no_wrap(g.to_owned(), fonte_icone(medidas.icone_p), cor_texto)
            });
            let largura_icone = gi.as_ref().map_or(0.0, |g| g.size().x + 6.0);
            let largura = rotulo.size().x + largura_icone + 12.0;
            let (rect, resp) =
                ui.allocate_exact_size(vec2(largura, alt_linha), sentido(habilitada));
            pintar_fundo(ui, &resp, rect, acao, paleta, medidas);
            let mut x = rect.left() + 6.0;
            if let Some(g) = gi {
                painter.galley(
                    pos2(x, rect.center().y - g.size().y / 2.0),
                    g.clone(),
                    cor_texto,
                );
                x += g.size().x + 6.0;
            }
            painter.galley(
                pos2(x, rect.center().y - rotulo.size().y / 2.0),
                rotulo,
                cor_texto,
            );
            (rect, resp)
        }
        TamanhoBotao::Icone => {
            let texto = icone
                .map(str::to_owned)
                .unwrap_or_else(|| acao.rotulo.chars().take(2).collect::<String>());
            let fonte = if icone.is_some() {
                fonte_icone(medidas.icone_p)
            } else {
                FontId::proportional(pequeno)
            };
            let g = painter.layout_no_wrap(texto, fonte, cor_texto);
            let lado = alt_linha.max(g.size().x + 8.0);
            let (rect, resp) = ui.allocate_exact_size(vec2(lado, alt_linha), sentido(habilitada));
            pintar_fundo(ui, &resp, rect, acao, paleta, medidas);
            painter.galley(rect.center() - g.size() / 2.0, g, cor_texto);
            (rect, resp)
        }
    };
    let _ = rect;
    let resposta = resposta.on_hover_text(acao.texto_da_dica());
    if habilitada && resposta.clicked() {
        pedidos::executar(ui.ctx(), acao.id.clone());
    }
    resposta
}

fn sentido(habilitada: bool) -> Sense {
    if habilitada {
        Sense::click()
    } else {
        Sense::hover()
    }
}

fn pintar_fundo(ui: &Ui, resp: &Response, rect: Rect, acao: &Acao, p: &Paleta, m: &Medidas) {
    let canto = CornerRadius::same(m.raio_m as u8);
    let marcada = acao.tipo == TipoAcao::Alternancia && acao.marcada;
    let fundo = if !acao.esta_habilitada() {
        if marcada {
            p.superficie_3
        } else {
            Color32::TRANSPARENT
        }
    } else if resp.is_pointer_button_down_on() {
        p.superficie_4
    } else if marcada {
        p.destaque_sutil
    } else if resp.hovered() {
        p.superficie_3
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, canto, fundo);
    if marcada {
        ui.painter().rect_stroke(
            rect,
            canto,
            Stroke::new(1.0, p.destaque_borda),
            egui::StrokeKind::Inside,
        );
    }
    if resp.has_focus() {
        ui.painter().rect_stroke(
            rect.expand(1.0),
            canto,
            Stroke::new(2.0, p.anel_de_foco),
            egui::StrokeKind::Outside,
        );
    }
}

/// Um quadro da janela inteira.
pub(crate) fn desenhar_janela<A: AppCasca>(casca: &mut Casca<A>, ui: &mut Ui, q: &Quadro) {
    // A Ui raiz do eframe não tem fundo: pinta a janela inteira com `surface-0`.
    let fundo = casca.estado.tema.paleta.superficie_0;
    ui.painter()
        .rect_filled(ui.max_rect(), CornerRadius::ZERO, fundo);
    barra_de_titulo(casca, ui, q);
    let area_abas = desenhar_ribbon(casca, ui, q);
    barra_de_status(casca, ui);
    if casca.estado.ribbon.bastidores_abertos {
        bastidores(casca, ui);
    } else {
        painel_de_navegacao(casca, ui);
        dock(casca, ui);
        centro(casca, ui);
    }
    corpo_recolhido(casca, ui.ctx(), q, area_abas);
    flutuantes(casca, ui.ctx());
    desenhar_paleta(casca, ui.ctx(), q);
    mapa_de_atalhos(casca, ui.ctx(), q);
    keytips(casca, ui.ctx());
}

fn barra_de_titulo<A: AppCasca>(casca: &mut Casca<A>, ui: &mut Ui, q: &Quadro) {
    let tema = std::sync::Arc::clone(&casca.estado.tema);
    let (p, m) = (tema.paleta, tema.medidas);
    let mut pontos = Vec::new();
    egui::Panel::top("casca.titulo")
        .exact_size((m.altura_controle * 0.75).max(22.0) + 6.0)
        .frame(
            Frame::new()
                .fill(p.superficie_0)
                .inner_margin(Margin::symmetric(8, 3)),
        )
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (i, id) in q.acesso_rapido.iter().enumerate() {
                    if let Some(acao) = q.acoes.get(id) {
                        let r =
                            botao_acao(ui, acao, TamanhoBotao::Icone, &casca.estado.icones, &p, &m);
                        if i < 9 {
                            pontos.push(((i + 1).to_string(), r.rect));
                        }
                    }
                }
                ui.add_space(12.0);
                let titulo = format!(
                    "{} — {}",
                    casca.app.titulo_da_rota(casca.estado.historico.atual()),
                    casca.estado.titulo
                );
                ui.label(RichText::new(titulo).small().color(p.tinta_suave));
            });
        });
    if casca.estado.keytips.nivel() == &NivelKeyTip::Guias {
        casca.estado.pontos_keytip.extend(pontos);
    }
}

/// Desenha a faixa das abas e o corpo (clássico ou simples). Devolve o retângulo da faixa das
/// abas, para o corpo recolhido abrir logo abaixo.
fn desenhar_ribbon<A: AppCasca>(casca: &mut Casca<A>, ui: &mut Ui, q: &Quadro) -> Rect {
    let tema = std::sync::Arc::clone(&casca.estado.tema);
    let (p, m) = (tema.paleta, tema.medidas);
    let textos_arquivo = casca.estado.textos.t("casca-aba-arquivo");
    let ativa = casca.estado.ribbon.aba_efetiva(&q.guias).map(str::to_owned);
    let mut clique: Option<(Guia, bool)> = None;
    let mut pontos = Vec::new();
    let mut area_abas = Rect::NOTHING;
    let modo = casca.estado.ribbon.modo;
    let mostra_corpo = casca.estado.ribbon.mostra_corpo() && modo != ModoRibbon::Recolhido;
    let nivel = casca.estado.keytips.nivel().clone();
    // Altura fixa por modo: a faixa das abas mais o corpo (clássico ou simples).
    let corpo = if !mostra_corpo || ativa.is_none() {
        0.0
    } else if modo == ModoRibbon::Simples {
        m.altura_ribbon_simples + 1.0
    } else {
        m.altura_ribbon_classico + 1.0
    };
    egui::Panel::top("casca.ribbon")
        .exact_size(m.altura_abas_ribbon + corpo + 2.0)
        .frame(
            Frame::new()
                .fill(p.superficie_2)
                .inner_margin(Margin::symmetric(8, 0))
                .stroke(Stroke::new(1.0, p.borda)),
        )
        .show(ui, |ui| {
            // A altura do Ribbon é a dos tokens: sem respiro vertical extra entre as linhas.
            ui.spacing_mut().item_spacing.y = 0.0;
            let faixa = ui.horizontal(|ui| {
                ui.set_height(m.altura_abas_ribbon);
                ui.spacing_mut().item_spacing.x = 2.0;
                for guia in &q.guias {
                    let (rotulo, keytip, contextual) = match guia {
                        Guia::Arquivo => (textos_arquivo.clone(), KEYTIP_ARQUIVO.to_owned(), false),
                        Guia::Aba(id) => match q.abas.iter().find(|a| &a.id == id) {
                            Some(a) => (a.rotulo.clone(), a.keytip.clone(), a.contextual),
                            None => continue,
                        },
                    };
                    let selecionada = match guia {
                        Guia::Arquivo => casca.estado.ribbon.bastidores_abertos,
                        Guia::Aba(id) => {
                            !casca.estado.ribbon.bastidores_abertos
                                && casca.estado.ribbon.mostra_corpo()
                                && ativa.as_deref() == Some(id.as_str())
                        }
                    };
                    let r = guia_ribbon(
                        ui,
                        &rotulo,
                        selecionada,
                        *guia == Guia::Arquivo,
                        contextual,
                        &p,
                        &m,
                    );
                    if r.clicked() || r.double_clicked() {
                        clique = Some((guia.clone(), r.double_clicked()));
                    }
                    if !keytip.is_empty() {
                        pontos.push((keytip, r.rect));
                    }
                }
            });
            area_abas = faixa.response.rect;
            if mostra_corpo
                && let Some(id) = &ativa
                && let Some(aba) = q.abas.iter().find(|a| &a.id == id)
            {
                let (linha, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
                ui.painter().rect_filled(linha, CornerRadius::ZERO, p.borda);
                let pts = corpo_do_ribbon(ui, aba, modo, q, casca, &p, &m);
                if nivel == NivelKeyTip::Acoes(aba.id.clone()) {
                    casca.estado.pontos_keytip.extend(pts);
                }
            }
        });
    if let Some((guia, duplo)) = clique {
        // O duplo clique chega também como clique simples no quadro anterior: desfaz o efeito
        // do simples antes de alternar o recolhido.
        casca.estado.ribbon.clicar_guia(&guia, duplo);
    }
    if nivel == NivelKeyTip::Guias {
        casca.estado.pontos_keytip.extend(pontos);
    }
    area_abas
}

fn guia_ribbon(
    ui: &mut Ui,
    rotulo: &str,
    selecionada: bool,
    arquivo: bool,
    contextual: bool,
    p: &Paleta,
    m: &Medidas,
) -> Response {
    let fonte = FontId::proportional(
        ui.style()
            .text_styles
            .get(&egui::TextStyle::Body)
            .map_or(14.0, |f| f.size),
    );
    let cor = if arquivo {
        p.sobre_destaque
    } else if selecionada {
        p.destaque_tinta
    } else if contextual {
        p.info
    } else {
        p.tinta
    };
    let g = ui.painter().layout_no_wrap(rotulo.to_owned(), fonte, cor);
    let tam = vec2(g.size().x + 20.0, m.altura_abas_ribbon - 4.0);
    let (rect, resp) = ui.allocate_exact_size(tam, Sense::click());
    let canto = CornerRadius::same(m.raio_m as u8);
    if arquivo {
        let fundo = if resp.hovered() {
            p.destaque_hover
        } else {
            p.destaque
        };
        ui.painter().rect_filled(rect, canto, fundo);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, canto, p.superficie_3);
    }
    if contextual {
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min, vec2(rect.width(), 3.0)),
            CornerRadius::ZERO,
            p.info,
        );
    }
    ui.painter().galley(rect.center() - g.size() / 2.0, g, cor);
    if selecionada && !arquivo {
        ui.painter().rect_filled(
            Rect::from_min_max(
                pos2(rect.left() + 6.0, rect.bottom() - 2.0),
                pos2(rect.right() - 6.0, rect.bottom()),
            ),
            CornerRadius::same(1),
            p.destaque,
        );
    }
    resp
}

/// O corpo de uma aba. Devolve os KeyTips das Ações desenhadas.
fn corpo_do_ribbon<A: AppCasca>(
    ui: &mut Ui,
    aba: &ribbon::Aba,
    modo: ModoRibbon,
    q: &Quadro,
    casca: &mut Casca<A>,
    p: &Paleta,
    m: &Medidas,
) -> Vec<(String, Rect)> {
    let icones = casca.estado.icones.clone();
    let mut pontos = Vec::new();
    let anotar = |acao: &Acao, r: &Response, pontos: &mut Vec<(String, Rect)>| {
        if !acao.keytip.is_empty() {
            pontos.push((acao.keytip.clone(), r.rect));
        }
    };
    let simples = match modo {
        ModoRibbon::Simples => true,
        ModoRibbon::Classico => false,
        ModoRibbon::Recolhido => casca.estado.ribbon.modo_expandido == ModoRibbon::Simples,
    };
    if !simples {
        let alt_acoes = (m.altura_controle * 0.75).max(22.0) * 3.0;
        ui.horizontal_top(|ui| {
            for (n, grupo) in aba.grupos.iter().enumerate() {
                if n > 0 {
                    ui.separator();
                }
                ui.vertical(|ui| {
                    ui.horizontal_top(|ui| {
                        ui.set_min_height(alt_acoes.max(64.0));
                        ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
                        let mut acoes = grupo.acoes.iter().filter_map(|id| q.acoes.get(id));
                        if let Some(primeira) = acoes.next() {
                            let r = botao_acao(ui, primeira, TamanhoBotao::Grande, &icones, p, m);
                            anotar(primeira, &r, &mut pontos);
                        }
                        let resto: Vec<&Acao> = acoes.collect();
                        for coluna in resto.chunks(3) {
                            ui.vertical(|ui| {
                                for acao in coluna {
                                    let r =
                                        botao_acao(ui, acao, TamanhoBotao::Pequeno, &icones, p, m);
                                    anotar(acao, &r, &mut pontos);
                                }
                            });
                        }
                    });
                    ui.label(RichText::new(&grupo.rotulo).small().color(p.tinta_suave));
                });
            }
        });
        ui.add_space(2.0);
    } else {
        let disponivel = ui.available_width();
        let larguras: Vec<f32> = aba
            .grupos
            .iter()
            .map(|g| {
                casca
                    .estado
                    .larguras_grupos
                    .get(&format!("{}/{}", aba.id, g.id))
                    .copied()
                    .unwrap_or(80.0)
            })
            .collect();
        let corte = ribbon::cortar(&larguras, 10.0, disponivel, 40.0);
        let mut medidas_novas = HashMap::new();
        ui.horizontal(|ui| {
            ui.set_min_height(m.altura_ribbon_simples - 8.0);
            for (n, grupo) in aba.grupos.iter().enumerate().take(corte.visiveis) {
                if n > 0 {
                    ui.separator();
                }
                let r = ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for acao in grupo.acoes.iter().filter_map(|id| q.acoes.get(id)) {
                        let r = botao_acao(ui, acao, TamanhoBotao::Pequeno, &icones, p, m);
                        anotar(acao, &r, &mut pontos);
                    }
                });
                medidas_novas.insert(format!("{}/{}", aba.id, grupo.id), r.response.rect.width());
            }
            if !corte.excedentes.is_empty() {
                ui.separator();
                let mais = casca.estado.textos.t("casca-ribbon-mais");
                ui.menu_button(RichText::new("…").strong(), |ui| {
                    for &i in &corte.excedentes {
                        let grupo = &aba.grupos[i];
                        ui.label(RichText::new(&grupo.rotulo).small().color(p.tinta_suave));
                        for acao in grupo.acoes.iter().filter_map(|id| q.acoes.get(id)) {
                            let r = botao_acao(ui, acao, TamanhoBotao::Pequeno, &icones, p, m);
                            if r.clicked() {
                                ui.close();
                            }
                        }
                        ui.separator();
                    }
                })
                .response
                .on_hover_text(mais);
            }
        });
        casca.estado.larguras_grupos.extend(medidas_novas);
    }
    pontos
}

/// Modo recolhido: o corpo da aba abre por cima do conteúdo, logo abaixo das abas.
fn corpo_recolhido<A: AppCasca>(casca: &mut Casca<A>, ctx: &egui::Context, q: &Quadro, abas: Rect) {
    let e = &casca.estado.ribbon;
    if e.modo != ModoRibbon::Recolhido || !e.corpo_aberto || e.bastidores_abertos {
        return;
    }
    let Some(id) = e.aba_efetiva(&q.guias).map(str::to_owned) else {
        return;
    };
    let Some(aba) = q.abas.iter().find(|a| a.id == id) else {
        return;
    };
    let tema = std::sync::Arc::clone(&casca.estado.tema);
    let (p, m) = (tema.paleta, tema.medidas);
    let nivel = casca.estado.keytips.nivel().clone();
    let resp = egui::Area::new(Id::new("casca.ribbon.recolhido"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos2(abas.left(), abas.bottom() + 1.0))
        .show(ctx, |ui| {
            Frame::popup(ui.style())
                .fill(p.superficie_2)
                .show(ui, |ui| {
                    // Numa Area a altura é livre: limita à do corpo expandido (os separadores
                    // verticais ocupam toda a altura disponível).
                    let altura = if casca.estado.ribbon.modo_expandido == ModoRibbon::Simples {
                        m.altura_ribbon_simples
                    } else {
                        m.altura_ribbon_classico
                    };
                    ui.set_max_height(altura);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    corpo_do_ribbon(ui, aba, ModoRibbon::Recolhido, q, casca, &p, &m)
                })
                .inner
        });
    if nivel == NivelKeyTip::Acoes(aba.id.clone()) {
        casca.estado.pontos_keytip.extend(resp.inner);
    }
    let fora = resp.response.clicked_elsewhere()
        && !ctx
            .input(|i| i.pointer.interact_pos())
            .is_some_and(|pos| abas.contains(pos));
    if fora || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        casca.estado.ribbon.corpo_aberto = false;
    }
}

fn bastidores<A: AppCasca>(casca: &mut Casca<A>, ui: &mut Ui) {
    let tema = std::sync::Arc::clone(&casca.estado.tema);
    let (p, m) = (tema.paleta, tema.medidas);
    let secoes = casca.app.secoes_bastidores();
    let atual = casca
        .estado
        .secao_bastidores
        .clone()
        .filter(|s| secoes.iter().any(|x| &x.id == s))
        .or_else(|| secoes.first().map(|s| s.id.clone()));
    let voltar = casca.estado.textos.t("casca-bastidores-voltar");
    let icones = casca.estado.icones.clone();
    let mut escolhida = None;
    egui::CentralPanel::default()
        .frame(Frame::new().fill(p.superficie_1))
        .show(ui, |ui| {
            egui::Panel::left("casca.bastidores.menu")
                .exact_size(220.0)
                .frame(Frame::new().fill(p.destaque).inner_margin(Margin::same(8)))
                .show(ui, |ui| {
                    let (g, _) = glifo(&icones, "voltar", "⏴");
                    let r = ui.add(
                        egui::Button::new(
                            RichText::new(format!("{g}  {voltar}")).color(p.sobre_destaque),
                        )
                        .frame(false),
                    );
                    if r.clicked() {
                        pedidos::pedir(ui.ctx(), Pedido::FecharBastidores);
                    }
                    ui.add_space(12.0);
                    for s in &secoes {
                        let sel = atual.as_deref() == Some(s.id.as_str());
                        let texto = match icones.get(&s.icone) {
                            Some(g) => format!("{g}  {}", s.rotulo),
                            None => s.rotulo.clone(),
                        };
                        let rotulo = RichText::new(texto).color(p.sobre_destaque);
                        let rotulo = if sel { rotulo.strong() } else { rotulo };
                        let (rect, resp) = ui.allocate_exact_size(
                            vec2(ui.available_width(), m.altura_controle),
                            Sense::click(),
                        );
                        if sel || resp.hovered() {
                            ui.painter().rect_filled(
                                rect,
                                CornerRadius::same(m.raio_m as u8),
                                p.destaque_hover,
                            );
                        }
                        ui.painter().text(
                            pos2(rect.left() + 10.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            rotulo.text(),
                            FontId::proportional(14.0),
                            p.sobre_destaque,
                        );
                        if resp.clicked() {
                            escolhida = Some(s.id.clone());
                        }
                    }
                });
            egui::CentralPanel::default()
                .frame(
                    Frame::new()
                        .fill(p.superficie_1)
                        .inner_margin(Margin::same(24)),
                )
                .show(ui, |ui| {
                    if let Some(id) = &atual {
                        egui::ScrollArea::vertical()
                            .auto_shrink(false)
                            .show(ui, |ui| casca.app.desenhar_bastidores(ui, id));
                    }
                });
        });
    if let Some(id) = escolhida {
        casca.estado.secao_bastidores = Some(id);
    } else if casca.estado.secao_bastidores.is_none() {
        casca.estado.secao_bastidores = atual;
    }
}

fn painel_de_navegacao<A: AppCasca>(casca: &mut Casca<A>, ui: &mut Ui) {
    let tema = std::sync::Arc::clone(&casca.estado.tema);
    let (p, m) = (tema.paleta, tema.medidas);
    let grupos = casca.app.navegacao();
    let atual = casca.estado.historico.atual().clone();
    let ativo = navegacao::item_ativo(&grupos, &atual).map(|i| i.rota.clone());
    let icones = casca.estado.icones.clone();
    egui::Panel::left("casca.navegacao")
        .resizable(true)
        .default_size(232.0)
        .size_range(160.0..=420.0)
        .frame(
            Frame::new()
                .fill(p.superficie_1)
                .inner_margin(Margin::symmetric(6, 8))
                .stroke(Stroke::new(1.0, p.borda)),
        )
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    for grupo in &grupos {
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(&grupo.rotulo)
                                .small()
                                .strong()
                                .color(p.tinta_suave),
                        );
                        ui.add_space(2.0);
                        for item in &grupo.itens {
                            item_de_navegacao(
                                ui,
                                item,
                                ativo.as_ref() == Some(&item.rota),
                                &icones,
                                &p,
                                &m,
                            );
                        }
                    }
                });
        });
}

fn item_de_navegacao(
    ui: &mut Ui,
    item: &navegacao::ItemNavegacao,
    ativo: bool,
    icones: &FamiliaDeIcones,
    p: &Paleta,
    m: &Medidas,
) {
    let habilitado = item.indisponivel.is_none();
    let altura = (m.altura_controle * 0.875).max(24.0);
    let (rect, resp) =
        ui.allocate_exact_size(vec2(ui.available_width(), altura), sentido(habilitado));
    let canto = CornerRadius::same(m.raio_m as u8);
    if ativo {
        ui.painter().rect_filled(rect, canto, p.destaque_sutil);
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min + vec2(0.0, 4.0), vec2(3.0, rect.height() - 8.0)),
            CornerRadius::same(1),
            p.destaque,
        );
    } else if habilitado && resp.hovered() {
        ui.painter().rect_filled(rect, canto, p.superficie_3);
    }
    let cor = if !habilitado {
        p.tinta_desabilitada
    } else if ativo {
        p.destaque_tinta
    } else {
        p.tinta
    };
    let mut x = rect.left() + 10.0;
    if let Some(g) = icones.get(&item.icone) {
        let gal = ui.painter().layout_no_wrap(
            g.to_owned(),
            FontId::new(m.icone_p, icones.familia()),
            cor,
        );
        ui.painter()
            .galley(pos2(x, rect.center().y - gal.size().y / 2.0), gal, cor);
        x += m.icone_p + 8.0;
    }
    let fonte = FontId::proportional(13.5);
    let rotulo = ui.painter().layout_no_wrap(item.rotulo.clone(), fonte, cor);
    ui.painter().galley(
        pos2(x, rect.center().y - rotulo.size().y / 2.0),
        rotulo,
        cor,
    );
    let direita = rect.right() - 8.0;
    if let Some(ind) = &item.indisponivel {
        let selo = ui.painter().layout_no_wrap(
            ind.selo.clone(),
            FontId::proportional(11.0),
            p.tinta_suave,
        );
        let caixa = Rect::from_min_size(
            pos2(
                direita - selo.size().x - 10.0,
                rect.center().y - selo.size().y / 2.0 - 2.0,
            ),
            selo.size() + vec2(10.0, 4.0),
        );
        ui.painter()
            .rect_filled(caixa, CornerRadius::same(255), p.superficie_3);
        ui.painter()
            .galley(caixa.min + vec2(5.0, 2.0), selo, p.tinta_suave);
    } else if let Some(n) = item.contagem {
        ui.painter().text(
            pos2(direita, rect.center().y),
            Align2::RIGHT_CENTER,
            n.to_string(),
            FontId::proportional(12.0),
            p.tinta_suave,
        );
    }
    let resp = match &item.indisponivel {
        Some(ind) => resp.on_hover_text(&ind.motivo),
        None => resp,
    };
    if habilitado && resp.clicked() {
        let ctrl = ui.input(|i| i.modifiers.command);
        pedidos::pedir(
            ui.ctx(),
            pedidos::pedido_do_clique(item.rota.clone(), Local::Centro, ctrl),
        );
    } else if habilitado && resp.middle_clicked() {
        pedidos::abrir_flutuante(ui.ctx(), item.rota.clone());
    }
}

/// Voltar, Avançar e a trilha de um lugar.
fn barra_da_trilha<A: AppCasca>(
    ui: &mut Ui,
    app: &A,
    historico: &navegacao::Historico,
    local: Local,
    icones: &FamiliaDeIcones,
    p: &Paleta,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let (gv, _) = glifo(icones, "voltar", "⏴");
        let (ga, _) = glifo(icones, "avancar", "⏵");
        let fonte = FontId::new(
            14.0,
            if icones.get("voltar").is_some() {
                icones.familia()
            } else {
                egui::FontFamily::Proportional
            },
        );
        let rv = ui.add_enabled(
            historico.pode_voltar(),
            egui::Button::new(RichText::new(gv).font(fonte.clone())).frame(false),
        );
        if rv.on_hover_text("Alt+←").clicked() {
            pedidos::pedir(ui.ctx(), Pedido::Voltar(local));
        }
        let ra = ui.add_enabled(
            historico.pode_avancar(),
            egui::Button::new(RichText::new(ga).font(fonte)).frame(false),
        );
        if ra.on_hover_text("Alt+→").clicked() {
            pedidos::pedir(ui.ctx(), Pedido::Avancar(local));
        }
        ui.add_space(6.0);
        let trilha = app.trilha(historico.atual());
        let n = trilha.len();
        for (i, elo) in trilha.into_iter().enumerate() {
            if i > 0 {
                ui.label(RichText::new("›").color(p.tinta_suave));
            }
            let ultimo = i + 1 == n;
            match elo.rota {
                Some(rota) if !ultimo => {
                    pedidos::link_item(ui, elo.rotulo, rota);
                }
                _ => {
                    let t = RichText::new(elo.rotulo);
                    ui.label(if ultimo {
                        t.strong().color(p.tinta)
                    } else {
                        t.color(p.tinta_suave)
                    });
                }
            }
        }
    });
}

fn centro<A: AppCasca>(casca: &mut Casca<A>, ui: &mut Ui) {
    let tema = std::sync::Arc::clone(&casca.estado.tema);
    let p = tema.paleta;
    let rota = casca.estado.historico.atual().clone();
    let icones = casca.estado.icones.clone();
    egui::CentralPanel::default()
        .frame(
            Frame::new()
                .fill(p.superficie_0)
                .inner_margin(Margin::symmetric(12, 6)),
        )
        .show(ui, |ui| {
            pedidos::definir_local(ui.ctx(), Local::Centro);
            barra_da_trilha(
                ui,
                &casca.app,
                &casca.estado.historico,
                Local::Centro,
                &icones,
                &p,
            );
            ui.separator();
            casca.app.desenhar_pagina(ui, &rota, Local::Centro);
        });
}

struct ComportamentoDock<'a, A> {
    app: &'a mut A,
    titulos: &'a HashMap<String, String>,
    altura_aba: f32,
    paleta: Paleta,
}

impl<A: AppCasca> egui_tiles::Behavior<String> for ComportamentoDock<'_, A> {
    fn pane_ui(
        &mut self,
        ui: &mut Ui,
        _tile: egui_tiles::TileId,
        pane: &mut String,
    ) -> egui_tiles::UiResponse {
        Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt(("casca.dock", pane.as_str()))
                .auto_shrink(false)
                .show(ui, |ui| self.app.desenhar_painel(ui, pane));
        });
        egui_tiles::UiResponse::None
    }

    fn tab_title_for_pane(&mut self, pane: &String) -> egui::WidgetText {
        self.titulos
            .get(pane)
            .cloned()
            .unwrap_or_else(|| pane.clone())
            .into()
    }

    fn tab_bar_height(&self, _style: &egui::Style) -> f32 {
        self.altura_aba
    }

    fn tab_bar_color(&self, _visuals: &egui::Visuals) -> Color32 {
        self.paleta.superficie_2
    }

    fn simplification_options(&self) -> egui_tiles::SimplificationOptions {
        egui_tiles::SimplificationOptions {
            all_panes_must_have_tabs: true,
            ..Default::default()
        }
    }
}

/// Monta a árvore do dock: uma pilha de abas por número de `pilha`, empilhadas de cima para
/// baixo.
pub(crate) fn montar_dock(paineis: &[crate::app::PainelDock]) -> egui_tiles::Tree<String> {
    let mut tiles = egui_tiles::Tiles::default();
    let mut pilhas: Vec<u8> = paineis.iter().map(|p| p.pilha).collect();
    pilhas.sort_unstable();
    pilhas.dedup();
    let grupos: Vec<egui_tiles::TileId> = pilhas
        .iter()
        .map(|n| {
            let filhos = paineis
                .iter()
                .filter(|p| p.pilha == *n)
                .map(|p| tiles.insert_pane(p.id.clone()))
                .collect();
            tiles.insert_tab_tile(filhos)
        })
        .collect();
    let raiz = if grupos.len() == 1 {
        grupos[0]
    } else {
        tiles.insert_vertical_tile(grupos)
    };
    egui_tiles::Tree::new("casca.dock", raiz, tiles)
}

fn dock<A: AppCasca>(casca: &mut Casca<A>, ui: &mut Ui) {
    let paineis = casca.app.paineis();
    if paineis.is_empty() {
        return;
    }
    let assinatura: Vec<String> = paineis.iter().map(|p| p.id.clone()).collect();
    if casca.estado.dock.is_none() || casca.estado.assinatura_dock != assinatura {
        casca.estado.dock = Some(montar_dock(&paineis));
        casca.estado.assinatura_dock = assinatura;
        for p in &paineis {
            casca
                .estado
                .paineis_visiveis
                .entry(p.id.clone())
                .or_insert(p.visivel_por_padrao);
        }
    }
    let algum = paineis.iter().any(|p| casca.painel_visivel(&p.id));
    if !algum {
        return;
    }
    let tema = std::sync::Arc::clone(&casca.estado.tema);
    let (p, m) = (tema.paleta, tema.medidas);
    let titulos: HashMap<String, String> = paineis
        .iter()
        .map(|x| (x.id.clone(), x.titulo.clone()))
        .collect();
    let visiveis = casca.estado.paineis_visiveis.clone();
    let Some(arvore) = casca.estado.dock.as_mut() else {
        return;
    };
    let ids: Vec<(egui_tiles::TileId, String)> = arvore
        .tiles
        .iter()
        .filter_map(|(tid, t)| match t {
            egui_tiles::Tile::Pane(id) => Some((*tid, id.clone())),
            egui_tiles::Tile::Container(_) => None,
        })
        .collect();
    for (tid, id) in ids {
        arvore.set_visible(tid, visiveis.get(&id).copied().unwrap_or(false));
    }
    let app = &mut casca.app;
    egui::Panel::right("casca.dock")
        .resizable(true)
        .default_size(340.0)
        .size_range(220.0..=720.0)
        .frame(
            Frame::new()
                .fill(p.superficie_1)
                .stroke(Stroke::new(1.0, p.borda)),
        )
        .show(ui, |ui| {
            pedidos::definir_local(ui.ctx(), Local::Centro);
            let mut comportamento = ComportamentoDock {
                app,
                titulos: &titulos,
                altura_aba: m.altura_cabecalho_painel,
                paleta: p,
            };
            arvore.ui(&mut comportamento, ui);
        });
}

fn barra_de_status<A: AppCasca>(casca: &mut Casca<A>, ui: &mut Ui) {
    let tema = std::sync::Arc::clone(&casca.estado.tema);
    let (p, m) = (tema.paleta, tema.medidas);
    let segmentos = casca.app.status();
    let erros = casca.estado.erros.clone();
    let icones = casca.estado.icones.clone();
    let mut clicado = None;
    egui::Panel::bottom("casca.status")
        .exact_size(m.altura_status)
        .frame(
            Frame::new()
                .fill(p.superficie_2)
                .inner_margin(Margin::symmetric(8, 0))
                .stroke(Stroke::new(1.0, p.borda)),
        )
        .show(ui, |ui| {
            // Controles da barra cabem na altura dela (o padrão do egui é 32 px).
            ui.spacing_mut().interact_size.y = m.altura_status - 4.0;
            ui.spacing_mut().button_padding.y = 0.0;
            ui.horizontal_centered(|ui| {
                let mut desenhar = |ui: &mut Ui, s: &crate::app::SegmentoStatus| {
                    let cor = match s.tom {
                        Tom::Neutro => p.tinta_suave,
                        Tom::Info => p.info,
                        Tom::Sucesso => p.sucesso,
                        Tom::Aviso => p.aviso,
                        Tom::Perigo => p.perigo,
                    };
                    // Ícone e texto em rótulos separados: a fonte do ícone tem outras métricas
                    // e, no mesmo texto, desalinharia a linha de base.
                    if let Some(g) = icones.get(&s.icone) {
                        ui.add(
                            egui::Label::new(
                                RichText::new(g)
                                    .font(FontId::new(11.0, icones.familia()))
                                    .color(cor),
                            )
                            .selectable(false),
                        );
                    }
                    let rt = RichText::new(&s.texto).small().color(cor);
                    let r = if s.clicavel {
                        ui.add(egui::Button::new(rt).frame(false))
                    } else {
                        ui.label(rt)
                    };
                    let r = if s.dica.is_empty() {
                        r
                    } else {
                        r.on_hover_text(&s.dica)
                    };
                    if s.clicavel && r.clicked() {
                        clicado = Some(s.id.clone());
                    }
                };
                for s in segmentos.iter().filter(|s| !s.a_direita) {
                    desenhar(ui, s);
                    ui.label(RichText::new("·").small().color(p.borda_forte));
                }
                for erro in &erros {
                    ui.label(RichText::new(format!("⚠ {erro}")).small().color(p.perigo));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    for s in segmentos.iter().filter(|s| s.a_direita) {
                        desenhar(ui, s);
                    }
                });
            });
        });
    if let Some(id) = clicado {
        casca.app.ao_clicar_status(ui.ctx(), &id);
    }
}

fn flutuantes<A: AppCasca>(casca: &mut Casca<A>, ctx: &egui::Context) {
    let tema = std::sync::Arc::clone(&casca.estado.tema);
    let p = tema.paleta;
    let icones = casca.estado.icones.clone();
    let t_centro = casca.estado.textos.t("casca-flutuante-abrir-no-centro");
    let t_destacar = casca.estado.textos.t("casca-flutuante-destacar");
    let t_recolocar = casca.estado.textos.t("casca-flutuante-recolocar");
    for id in casca.estado.flutuantes.ids() {
        let Some(painel) = casca.estado.flutuantes.get(id).cloned() else {
            continue;
        };
        let rota = painel.historico.atual().clone();
        let titulo = casca.app.titulo_da_rota(&rota);
        let local = Local::Flutuante(id);
        let mut acao_cabecalho: Option<bool> = None;
        let app = &mut casca.app;
        let conteudo = |ui: &mut Ui, app: &mut A, acao: &mut Option<bool>| {
            ui.horizontal(|ui| {
                barra_da_trilha(ui, &*app, &painel.historico, local, &icones, &p);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let rotulo = if painel.destacado {
                        &t_recolocar
                    } else {
                        &t_destacar
                    };
                    if ui.small_button(rotulo).clicked() {
                        *acao = Some(!painel.destacado);
                    }
                    if ui.small_button(&t_centro).clicked() {
                        pedidos::pedir(
                            ui.ctx(),
                            Pedido::Abrir {
                                rota: rota.clone(),
                                local: Local::Centro,
                            },
                        );
                    }
                });
            });
            ui.separator();
            pedidos::definir_local(ui.ctx(), local);
            app.desenhar_pagina(ui, &rota, local);
            pedidos::definir_local(ui.ctx(), Local::Centro);
        };
        let mut fechar = false;
        if painel.destacado {
            let vid = egui::ViewportId::from_hash_of(("casca.flutuante", id));
            let construtor = egui::ViewportBuilder::default()
                .with_title(titulo.clone())
                .with_inner_size([560.0, 640.0]);
            let recolocar = ctx.show_viewport_immediate(vid, construtor, |ui, classe| {
                match classe {
                    egui::ViewportClass::EmbeddedWindow => conteudo(ui, app, &mut acao_cabecalho),
                    _ => {
                        egui::CentralPanel::default()
                            .frame(
                                Frame::new()
                                    .fill(p.superficie_1)
                                    .inner_margin(Margin::same(10)),
                            )
                            .show(ui, |ui| conteudo(ui, app, &mut acao_cabecalho));
                    }
                }
                ui.input(|i| i.viewport().close_requested())
            });
            if recolocar {
                acao_cabecalho = Some(false);
            }
        } else {
            let mut aberto = true;
            // Em cascata a partir do centro da área livre, para não cobrir o Ribbon.
            let area = ctx.content_rect();
            let passo = (id % 6) as f32 * 28.0;
            egui::Window::new(titulo)
                .id(Id::new(("casca.flutuante", id)))
                .default_pos(pos2(
                    area.center().x - 160.0 + passo,
                    area.top() + 190.0 + passo,
                ))
                .open(&mut aberto)
                .default_size([480.0, 520.0])
                .resizable(true)
                .collapsible(false)
                .show(ctx, |ui| conteudo(ui, app, &mut acao_cabecalho));
            fechar = !aberto;
        }
        if let Some(destacar) = acao_cabecalho {
            casca.estado.flutuantes.destacar(id, destacar);
        }
        if fechar {
            casca.estado.flutuantes.fechar(id);
        }
    }
}

fn desenhar_paleta<A: AppCasca>(casca: &mut Casca<A>, ctx: &egui::Context, q: &Quadro) {
    if !casca.estado.paleta.aberta {
        return;
    }
    let tema = std::sync::Arc::clone(&casca.estado.tema);
    let (p, m) = (tema.paleta, tema.medidas);
    let entradas = paleta::entradas(
        &q.acoes,
        &casca.app.navegacao(),
        &casca.app.destinos(),
        &casca.estado.textos.t("casca-paleta-area"),
    );
    let dica = casca.estado.textos.t("casca-paleta-busca");
    let icones = casca.estado.icones.clone();
    let estado = &mut casca.estado.paleta;
    let mostradas = paleta::visiveis(&entradas, &estado.busca);
    let (baixo, cima, enter, ctrl) = ctx.input_mut(|i| {
        (
            i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
            i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
            i.key_pressed(egui::Key::Enter),
            i.modifiers.command,
        )
    });
    if baixo {
        estado.mover(1, mostradas.len());
    }
    if cima {
        estado.mover(-1, mostradas.len());
    }
    let mut ativar_linha: Option<usize> = enter.then_some(estado.selecionada);
    let vazia = casca.estado.textos.t_com(
        "casca-paleta-vazia",
        &[("busca", &casca.estado.paleta.busca)],
    );
    let estado = &mut casca.estado.paleta;
    let resposta = egui::Modal::new(Id::new("casca.paleta"))
        .frame(
            Frame::popup(&ctx.global_style())
                .fill(p.superficie_1)
                .corner_radius(CornerRadius::same(m.raio_xl as u8)),
        )
        .show(ctx, |ui| {
            ui.set_width(560.0);
            let campo = ui.add(
                egui::TextEdit::singleline(&mut estado.busca)
                    .hint_text(dica)
                    .font(egui::TextStyle::Name(crate::tema::CORPO_GRANDE.into()))
                    .desired_width(f32::INFINITY),
            );
            campo.request_focus();
            if campo.changed() {
                estado.selecionada = 0;
            }
            ui.add_space(4.0);
            if mostradas.is_empty() {
                ui.label(RichText::new(vazia).color(p.tinta_suave));
            }
            egui::ScrollArea::vertical()
                .max_height(380.0)
                .show(ui, |ui| {
                    for (linha, &i) in mostradas.iter().enumerate().take(80) {
                        let e = &entradas[i];
                        let sel = linha == estado.selecionada;
                        let (rect, r) = ui.allocate_exact_size(
                            vec2(ui.available_width(), m.altura_controle),
                            Sense::click(),
                        );
                        if sel || r.hovered() {
                            ui.painter().rect_filled(
                                rect,
                                CornerRadius::same(m.raio_m as u8),
                                if sel {
                                    p.destaque_sutil
                                } else {
                                    p.superficie_3
                                },
                            );
                        }
                        if sel {
                            ui.scroll_to_rect(rect, None);
                        }
                        let cor = if e.habilitada() {
                            p.tinta
                        } else {
                            p.tinta_desabilitada
                        };
                        let mut x = rect.left() + 8.0;
                        if let Some(g) = icones.get(&e.icone) {
                            ui.painter().text(
                                pos2(x, rect.center().y),
                                Align2::LEFT_CENTER,
                                g,
                                FontId::new(m.icone_p, icones.familia()),
                                cor,
                            );
                        }
                        x += m.icone_p + 8.0;
                        ui.painter().text(
                            pos2(x, rect.center().y),
                            Align2::LEFT_CENTER,
                            &e.rotulo,
                            FontId::proportional(14.0),
                            cor,
                        );
                        let detalhe = match &e.motivo {
                            Some(motivo) => motivo.clone(),
                            None => e.detalhe.clone(),
                        };
                        ui.painter().text(
                            pos2(rect.right() - 8.0, rect.center().y),
                            Align2::RIGHT_CENTER,
                            detalhe,
                            FontId::proportional(12.0),
                            p.tinta_suave,
                        );
                        if r.clicked() {
                            ativar_linha = Some(linha);
                        }
                    }
                });
        });
    if let Some(linha) = ativar_linha
        && let Some(alvo) = paleta::ativar(&entradas, &mostradas, linha)
    {
        match alvo {
            Alvo::Acao(id) => pedidos::executar(ctx, id),
            Alvo::Destino(rota) => {
                pedidos::pedir(ctx, pedidos::pedido_do_clique(rota, Local::Centro, ctrl))
            }
        }
        casca.estado.paleta.fechar();
    }
    if resposta.should_close() {
        casca.estado.paleta.fechar();
    }
}

fn mapa_de_atalhos<A: AppCasca>(casca: &mut Casca<A>, ctx: &egui::Context, q: &Quadro) {
    if !casca.estado.mapa_de_atalhos_aberto {
        return;
    }
    let tema = std::sync::Arc::clone(&casca.estado.tema);
    let p = tema.paleta;
    let titulo = casca.estado.textos.t("casca-mapa-atalhos");
    let da_janela = casca.estado.textos.t("casca-mapa-atalhos-casca");
    let mapa = q.acoes.mapa_de_atalhos();
    let resposta = egui::Modal::new(Id::new("casca.mapa-atalhos")).show(ctx, |ui| {
        ui.set_width(460.0);
        ui.heading(titulo);
        ui.add_space(8.0);
        egui::ScrollArea::vertical().max_height(480.0).show(ui, |ui| {
            for (cabecalho, da_casca) in [(da_janela.as_str(), true), (casca.estado.titulo.as_str(), false)] {
                ui.label(RichText::new(cabecalho).small().strong().color(p.tinta_suave));
                egui::Grid::new(("casca.mapa", da_casca)).striped(true).num_columns(2).show(ui, |ui| {
                    for (atalho, rotulo, id) in mapa.iter().filter(|(_, _, id)| id.starts_with("casca.") == da_casca) {
                        ui.label(RichText::new(atalho.to_string()).monospace());
                        ui.label(*rotulo).on_hover_text(*id);
                        ui.end_row();
                    }
                });
                ui.add_space(8.0);
            }
            // KeyTips e mouse, que não são Ações.
            ui.label(RichText::new("Alt · KeyTips   |   Mouse ◀ ▶ · Voltar / Avançar   |   Ctrl+clique · painel flutuante").small().color(p.tinta_suave));
        });
    });
    if resposta.should_close() {
        casca.estado.mapa_de_atalhos_aberto = false;
    }
}

fn keytips<A: AppCasca>(casca: &mut Casca<A>, ctx: &egui::Context) {
    if !casca.estado.keytips.ativo() {
        return;
    }
    let p = casca.estado.tema.paleta;
    let digitado = casca.estado.keytips.digitado().to_owned();
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        Id::new("casca.keytips"),
    ));
    for (letras, rect) in &casca.estado.pontos_keytip {
        if !letras.starts_with(&digitado) {
            continue;
        }
        let g = painter.layout_no_wrap(letras.clone(), FontId::proportional(11.0), p.superficie_0);
        let caixa = Rect::from_center_size(
            pos2(rect.center().x, rect.bottom() - 2.0),
            g.size() + vec2(8.0, 4.0),
        );
        painter.rect_filled(caixa, CornerRadius::same(3), p.tinta);
        painter.galley(caixa.min + vec2(4.0, 2.0), g, p.superficie_0);
    }
}

/// Seletor de Tema, Densidade e Escala, para o app pôr na seção de Opções dos bastidores. Lê o
/// estado do `Context` e enfileira os pedidos de troca.
pub fn opcoes_de_aparencia(ui: &mut Ui, textos: &crate::i18n::Textos) {
    let (Some(ativo), Some(catalogo)) = (
        TemaAtivo::do_contexto(ui.ctx()),
        catalogo_do_contexto(ui.ctx()),
    ) else {
        return;
    };
    ui.heading(textos.t("casca-aparencia-tema"));
    let mut familias: Vec<&str> = Vec::new();
    for d in catalogo.descricoes() {
        if !familias.contains(&d.familia.as_str()) {
            familias.push(&d.familia);
        }
    }
    for familia in familias {
        ui.label(
            RichText::new(familia)
                .small()
                .color(ativo.paleta.tinta_suave),
        );
        ui.horizontal_wrapped(|ui| {
            for d in catalogo
                .descricoes()
                .iter()
                .filter(|d| d.familia == familia)
            {
                let sel = d.id == ativo.tema.id;
                let r = ui
                    .add(egui::Button::selectable(sel, &d.nome))
                    .on_hover_text(&d.inspiracao);
                if r.clicked() && !sel {
                    pedidos::pedir(ui.ctx(), Pedido::Tema(d.id.clone()));
                }
            }
        });
    }
    ui.add_space(12.0);
    ui.heading(textos.t("casca-aparencia-densidade"));
    ui.horizontal(|ui| {
        for (d, rotulo) in [
            (
                Densidade::Confortavel,
                textos.t("casca-aparencia-confortavel"),
            ),
            (Densidade::Compacta, textos.t("casca-aparencia-compacta")),
        ] {
            if ui
                .add(egui::Button::selectable(ativo.densidade == d, rotulo))
                .clicked()
            {
                pedidos::pedir(ui.ctx(), Pedido::Densidade(d));
            }
        }
    });
    ui.add_space(12.0);
    ui.heading(textos.t("casca-aparencia-escala"));
    ui.horizontal(|ui| {
        for e in ESCALAS {
            let sel = (ativo.escala - e).abs() < 0.001;
            if ui
                .add(egui::Button::selectable(sel, format!("{:.0}%", e * 100.0)))
                .clicked()
            {
                pedidos::pedir(ui.ctx(), Pedido::Escala(e));
            }
        }
    });
}
