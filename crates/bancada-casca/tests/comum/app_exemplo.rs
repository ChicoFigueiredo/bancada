//! Um app de exemplo, genérico e com dados fictícios, que exercita a casca inteira. Usado pelo
//! `examples/demonstracao.rs` e pelos testes sem janela.

#![allow(dead_code)]

use bancada_casca::egui::{self, RichText};
use bancada_casca::{
    Aba, Acao, AppCasca, Atalho, ConfigCasca, Destino, Elo, FamiliaDeIcones, Grupo, GrupoNavegacao,
    ItemNavegacao, Local, PainelDock, Rota, SecaoBastidores, SegmentoStatus, Textos, Tom, ids,
    link_item,
};

pub const TOKENS: &str = include_str!("../../dados-de-exemplo/tokens.json");
pub const TEMAS: &str = include_str!("../../dados-de-exemplo/temas.json");

/// Recursos Fluent do app de exemplo (mostra que o app acrescenta os dele).
const FTL: &str = "exemplo-titulo = Demonstração da bancada\n";

pub struct Projeto {
    pub id: &'static str,
    pub nome: &'static str,
    pub contato: &'static str,
    pub tarefas: &'static [(&'static str, &'static str)],
}

pub const CONTATOS: &[(&str, &str)] = &[
    ("ana", "Ana Exemplo"),
    ("bruno", "Bruno Fictício"),
    ("carla", "Carla Modelo"),
];

pub const PROJETOS: &[Projeto] = &[
    Projeto {
        id: "jardim",
        nome: "Jardim comunitário",
        contato: "ana",
        tarefas: &[("t1", "Comprar sementes"), ("t2", "Montar canteiros")],
    },
    Projeto {
        id: "oficina",
        nome: "Oficina de bicicletas",
        contato: "bruno",
        tarefas: &[("t3", "Inventário de peças")],
    },
    Projeto {
        id: "biblioteca",
        nome: "Biblioteca de bairro",
        contato: "ana",
        tarefas: &[("t4", "Catalogar doações"), ("t5", "Escala de voluntários")],
    },
];

/// O que o app de exemplo registra dos eventos (os testes conferem).
#[derive(Debug, Default)]
pub struct AppExemplo {
    pub selecionado: Option<String>,
    pub executadas: Vec<String>,
    pub rotas_abertas: Vec<(Rota, Local)>,
    pub cliques_status: Vec<String>,
    pub textos: Textos,
}

impl AppExemplo {
    pub fn novo() -> Self {
        let mut textos = Textos::pt_br();
        // O recurso do exemplo é fixo e válido.
        let _ = textos.acrescentar(FTL);
        Self {
            textos,
            ..Default::default()
        }
    }

    pub fn config() -> ConfigCasca {
        let mut c = ConfigCasca::nova("Demonstração da bancada", TOKENS, TEMAS);
        c.tema_inicial = "salvia".into();
        c.icones = icones();
        c
    }
}

/// Glifos que existem nas fontes padrão do egui (a casca não embute fonte de ícones).
pub fn icones() -> FamiliaDeIcones {
    FamiliaDeIcones::sem_fonte().glifos([
        ("voltar", "⏴"),
        ("avancar", "⏵"),
        ("paleta", "🔍"),
        ("atalhos", "⌨"),
        ("recolher", "⏶"),
        ("ribbon-classico", "☰"),
        ("ribbon-simples", "▬"),
        ("densidade", "↕"),
        ("novo", "➕"),
        ("excluir", "🗑"),
        ("concluir", "✔"),
        ("duvida", "❓"),
        ("atualizar", "🔄"),
        ("desfazer", "⟲"),
        ("enviar", "📤"),
        ("abrir", "📂"),
        ("copiar", "📋"),
        ("contato", "👤"),
        ("projeto", "📁"),
        ("tarefa", "📄"),
        ("inicio", "🏠"),
        ("relatorio", "📊"),
        ("detalhe", "ℹ"),
        ("historico", "🕓"),
        ("aviso", "⚠"),
        ("cadeado", "🔒"),
        ("opcoes", "⚙"),
    ])
}

fn nome_do_contato(id: &str) -> &'static str {
    CONTATOS
        .iter()
        .find(|(c, _)| *c == id)
        .map_or("?", |(_, n)| n)
}

fn projeto(id: &str) -> Option<&'static Projeto> {
    PROJETOS.iter().find(|p| p.id == id)
}

fn tarefa(id: &str) -> Option<(&'static Projeto, &'static str)> {
    PROJETOS.iter().find_map(|p| {
        p.tarefas
            .iter()
            .find(|(t, _)| *t == id)
            .map(|(_, nome)| (p, *nome))
    })
}

impl AppCasca for AppExemplo {
    fn acoes(&self) -> Vec<Acao> {
        let sel = self.selecionado.is_some();
        let motivo = "Selecione um item primeiro";
        vec![
            Acao::nova("item.novo", "Novo item")
                .icone("novo")
                .atalho(Atalho::ctrl(egui::Key::N))
                .keytip("N")
                .dica("Cria um item de exemplo"),
            Acao::nova("item.concluir", "Concluir")
                .icone("concluir")
                .keytip("C")
                .habilitada_se(sel, motivo),
            Acao::nova("item.duvida", "Em dúvida")
                .icone("duvida")
                .keytip("UD")
                .habilitada_se(sel, motivo),
            Acao::nova("item.excluir", "Excluir")
                .icone("excluir")
                .atalho(Atalho::tecla(egui::Key::Delete))
                .keytip("X")
                .habilitada_se(sel, motivo),
            Acao::nova("geral.atualizar", "Atualizar")
                .icone("atualizar")
                .atalho(Atalho::tecla(egui::Key::F5))
                .keytip("A"),
            Acao::nova("geral.desfazer", "Desfazer")
                .icone("desfazer")
                .atalho(Atalho::ctrl(egui::Key::Z))
                .keytip("D"),
            Acao::nova("lote.enviar", "Enviar ao servidor")
                .icone("enviar")
                .keytip("E")
                .desabilitada("Chega numa fatia futura (exemplo de Ação desabilitada com motivo)"),
            Acao::nova("item.abrir", "Abrir original")
                .icone("abrir")
                .keytip("O")
                .habilitada_se(sel, motivo),
            Acao::nova("item.copiar", "Copiar id")
                .icone("copiar")
                .keytip("I")
                .habilitada_se(sel, motivo),
        ]
    }

    fn abas(&self) -> Vec<Aba> {
        vec![
            Aba::nova("inicio", "Início", "I")
                .grupo(Grupo::novo(
                    "geral",
                    "Geral",
                    &["geral.atualizar", "geral.desfazer", ids::PALETA],
                ))
                .grupo(Grupo::novo("itens", "Itens", &["item.novo"])),
            Aba::nova("triagem", "Triagem", "T")
                .grupo(Grupo::novo(
                    "triagem",
                    "Triagem",
                    &["item.concluir", "item.duvida", "item.excluir"],
                ))
                .grupo(Grupo::novo("lotes", "Lotes", &["lote.enviar"]))
                .grupo(Grupo::novo("historico", "Histórico", &["geral.desfazer"])),
            Aba::nova("exibir", "Exibir", "X")
                .grupo(Grupo::novo(
                    "paineis",
                    "Painéis",
                    &[
                        ids::painel("detalhe").as_str(),
                        ids::painel("origens").as_str(),
                        ids::painel("historico").as_str(),
                    ],
                ))
                .grupo(Grupo::novo(
                    "ribbon",
                    "Ribbon",
                    &[
                        ids::RIBBON_CLASSICO,
                        ids::RIBBON_SIMPLES,
                        ids::RIBBON_RECOLHER,
                    ],
                ))
                .grupo(Grupo::novo(
                    "janela",
                    "Janela",
                    &[ids::DENSIDADE_COMPACTA, ids::MAPA_DE_ATALHOS],
                )),
            Aba::nova("item", "Item", "M")
                .contextual(self.selecionado.is_some())
                .grupo(Grupo::novo("item", "Item", &["item.abrir", "item.copiar"])),
        ]
    }

    fn acesso_rapido(&self) -> Vec<String> {
        vec![
            "geral.desfazer".into(),
            ids::PALETA.into(),
            "geral.atualizar".into(),
        ]
    }

    fn navegacao(&self) -> Vec<GrupoNavegacao> {
        let tarefas: usize = PROJETOS.iter().map(|p| p.tarefas.len()).sum();
        vec![
            GrupoNavegacao::novo(
                "Acervo",
                vec![
                    ItemNavegacao::novo("inicio", "Início", "inicio"),
                    ItemNavegacao::novo("projetos", "Projetos", "projeto")
                        .contagem(PROJETOS.len() as u64),
                    ItemNavegacao::novo("contatos", "Contatos", "contato")
                        .contagem(CONTATOS.len() as u64),
                    ItemNavegacao::novo("tarefas", "Tarefas", "tarefa").contagem(tarefas as u64),
                ],
            ),
            GrupoNavegacao::novo(
                "Mais tarde",
                vec![
                    ItemNavegacao::novo("relatorios", "Relatórios", "relatorio")
                        .indisponivel("fatia 2", "Chega na fatia 2 (exemplo de área indisponível)"),
                ],
            ),
        ]
    }

    fn destinos(&self) -> Vec<Destino> {
        let mut d: Vec<Destino> = PROJETOS
            .iter()
            .map(|p| Destino::novo(format!("projeto:{}", p.id), p.nome, "Projeto"))
            .collect();
        d.extend(
            CONTATOS
                .iter()
                .map(|(id, n)| Destino::novo(format!("contato:{id}"), *n, "Contato")),
        );
        d
    }

    fn rota_inicial(&self) -> Rota {
        Rota::nova("inicio")
    }

    fn trilha(&self, rota: &Rota) -> Vec<Elo> {
        let texto = rota.como_texto();
        let (tipo, id) = texto.split_once(':').unwrap_or((texto, ""));
        match tipo {
            "inicio" => vec![Elo::texto("Início")],
            "projetos" => vec![Elo::texto("Projetos")],
            "contatos" => vec![Elo::texto("Contatos")],
            "tarefas" => vec![Elo::texto("Tarefas")],
            "projeto" => vec![
                Elo::link("Projetos", "projetos"),
                Elo::texto(projeto(id).map_or("?", |p| p.nome)),
            ],
            "contato" => vec![
                Elo::link("Contatos", "contatos"),
                Elo::texto(nome_do_contato(id)),
            ],
            "tarefa" => match tarefa(id) {
                Some((p, nome)) => vec![
                    Elo::link("Projetos", "projetos"),
                    Elo::link(p.nome, format!("projeto:{}", p.id)),
                    Elo::texto(nome),
                ],
                None => vec![Elo::texto("Tarefa")],
            },
            _ => vec![Elo::texto(texto)],
        }
    }

    fn desenhar_pagina(&mut self, ui: &mut egui::Ui, rota: &Rota, local: Local) {
        let texto = rota.como_texto().to_owned();
        let (tipo, id) = texto.split_once(':').unwrap_or((&texto, ""));
        let tema = bancada_casca::TemaAtivo::paleta_do_contexto(ui.ctx());
        let suave = tema.map_or(egui::Color32::GRAY, |p| p.tinta_suave);
        egui::ScrollArea::vertical()
            .id_salt(("pagina", format!("{local:?}")))
            .auto_shrink(false)
            .show(ui, |ui| match tipo {
                "inicio" => {
                    ui.heading("Início");
                    ui.label("Todo nome é link: clique abre aqui, Ctrl+clique abre num painel flutuante.");
                    ui.label(RichText::new("Dados fictícios.").color(suave));
                    for p in PROJETOS {
                        ui.horizontal(|ui| {
                            link_item(ui, p.nome, format!("projeto:{}", p.id));
                            ui.label("de");
                            link_item(ui, nome_do_contato(p.contato), format!("contato:{}", p.contato));
                        });
                    }
                }
                "projetos" => {
                    ui.heading("Projetos");
                    egui::Grid::new("projetos").striped(true).show(ui, |ui| {
                        for p in PROJETOS {
                            link_item(ui, p.nome, format!("projeto:{}", p.id));
                            link_item(ui, nome_do_contato(p.contato), format!("contato:{}", p.contato));
                            ui.label(format!("{} tarefas", p.tarefas.len()));
                            if ui.small_button("Selecionar").clicked() {
                                self.selecionado = Some(p.id.into());
                            }
                            ui.end_row();
                        }
                    });
                }
                "contatos" => {
                    ui.heading("Contatos");
                    for (cid, nome) in CONTATOS {
                        link_item(ui, *nome, format!("contato:{cid}"));
                    }
                }
                "tarefas" => {
                    ui.heading("Tarefas");
                    for p in PROJETOS {
                        for (tid, nome) in p.tarefas {
                            ui.horizontal(|ui| {
                                link_item(ui, *nome, format!("tarefa:{tid}"));
                                ui.label(RichText::new("·").color(suave));
                                link_item(ui, p.nome, format!("projeto:{}", p.id));
                            });
                        }
                    }
                }
                "projeto" => match projeto(id) {
                    Some(p) => {
                        ui.heading(p.nome);
                        ui.horizontal(|ui| {
                            ui.label("Responsável:");
                            link_item(ui, nome_do_contato(p.contato), format!("contato:{}", p.contato));
                        });
                        ui.separator();
                        ui.label(RichText::new("Tarefas").strong());
                        for (tid, nome) in p.tarefas {
                            link_item(ui, *nome, format!("tarefa:{tid}"));
                        }
                        if ui.button("Selecionar este projeto").clicked() {
                            self.selecionado = Some(p.id.into());
                        }
                    }
                    None => {
                        ui.label("Projeto não encontrado.");
                    }
                },
                "contato" => {
                    ui.heading(nome_do_contato(id));
                    ui.label(RichText::new("Projetos").strong());
                    for p in PROJETOS.iter().filter(|p| p.contato == id) {
                        link_item(ui, p.nome, format!("projeto:{}", p.id));
                    }
                }
                "tarefa" => match tarefa(id) {
                    Some((p, nome)) => {
                        ui.heading(nome);
                        ui.horizontal(|ui| {
                            ui.label("Projeto:");
                            link_item(ui, p.nome, format!("projeto:{}", p.id));
                        });
                    }
                    None => {
                        ui.label("Tarefa não encontrada.");
                    }
                },
                _ => {
                    ui.label(format!("Página desconhecida: {texto}"));
                }
            });
    }

    fn paineis(&self) -> Vec<PainelDock> {
        vec![
            PainelDock::novo("detalhe", "Detalhe", 0).icone("detalhe"),
            PainelDock::novo("origens", "Origens", 0).icone("projeto"),
            PainelDock::novo("historico", "Histórico", 1).icone("historico"),
        ]
    }

    fn desenhar_painel(&mut self, ui: &mut egui::Ui, id: &str) {
        match id {
            "detalhe" => match &self.selecionado {
                Some(s) => {
                    ui.label("Selecionado:");
                    if let Some(p) = projeto(s) {
                        link_item(ui, p.nome, format!("projeto:{}", p.id));
                    }
                    if ui.small_button("Limpar seleção").clicked() {
                        self.selecionado = None;
                    }
                }
                None => {
                    ui.label("Nada selecionado. A aba contextual Item aparece com uma seleção.");
                }
            },
            "origens" => {
                ui.label("Origens fictícias:");
                link_item(ui, "Ana Exemplo", "contato:ana");
            }
            _ => {
                for (i, a) in self.executadas.iter().rev().take(20).enumerate() {
                    ui.label(format!("{}. {a}", i + 1));
                }
            }
        }
    }

    fn status(&self) -> Vec<SegmentoStatus> {
        vec![
            SegmentoStatus::novo("acervo", format!("{} projetos", PROJETOS.len())).icone("projeto"),
            SegmentoStatus::novo("cofre", "Cofre fechado")
                .icone("cadeado")
                .dica("Exemplo de segmento clicável")
                .clicavel(),
            SegmentoStatus::novo("aviso", "Aviso de exemplo")
                .icone("aviso")
                .tom(Tom::Aviso)
                .a_direita(),
        ]
    }

    fn secoes_bastidores(&self) -> Vec<SecaoBastidores> {
        vec![
            SecaoBastidores::nova("acervo", "Acervo", "projeto"),
            SecaoBastidores::nova("opcoes", "Opções", "opcoes"),
            SecaoBastidores::nova("sobre", "Sobre", "detalhe"),
        ]
    }

    fn desenhar_bastidores(&mut self, ui: &mut egui::Ui, id: &str) {
        match id {
            "opcoes" => bancada_casca::vista::opcoes_de_aparencia(ui, &self.textos),
            "sobre" => {
                ui.heading(self.textos.t("exemplo-titulo"));
                ui.label("bancada-casca: Ribbon, Ações, Paleta, KeyTips, navegação, dock, status, temas e i18n.");
            }
            _ => {
                ui.heading("Acervo");
                ui.label("Seção desenhada pelo app.");
            }
        }
    }

    fn ao_executar(&mut self, _ctx: &egui::Context, id: &str) {
        self.executadas.push(id.to_owned());
    }

    fn ao_abrir_rota(&mut self, _ctx: &egui::Context, rota: &Rota, local: Local) {
        self.rotas_abertas.push((rota.clone(), local));
    }

    fn ao_clicar_status(&mut self, _ctx: &egui::Context, id: &str) {
        self.cliques_status.push(id.to_owned());
    }
}
