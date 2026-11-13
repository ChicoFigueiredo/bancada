//! A casca em si: o estado da janela (Ribbon, KeyTips, Paleta, histórico, flutuantes, dock,
//! Tema) e o laço de cada quadro. O desenho de cada parte fica em [`crate::vista`].

use crate::acoes::{Acao, ErroAcoes, IdAcao, ListaDeAcoes};
use crate::app::AppCasca;
use crate::atalhos::Atalho;
use crate::i18n::Textos;
use crate::icones::FamiliaDeIcones;
use crate::navegacao::{Flutuantes, Historico, Local, Rota};
use crate::paleta::EstadoPaleta;
use crate::pedidos::{self, Pedido};
use crate::ribbon::{self, EstadoRibbon, KeyTips, ModoRibbon, ResultadoKeyTip};
use crate::tema::{CatalogoDeTemas, Densidade, ErroTokens, TemaAtivo};
use egui::{Key, PointerButton};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// Ids das Ações da casca.
pub mod ids {
    pub const VOLTAR: &str = "casca.voltar";
    pub const AVANCAR: &str = "casca.avancar";
    pub const PALETA: &str = "casca.paleta";
    pub const MAPA_DE_ATALHOS: &str = "casca.mapa-atalhos";
    pub const RIBBON_RECOLHER: &str = "casca.ribbon.recolher";
    pub const RIBBON_CLASSICO: &str = "casca.ribbon.classico";
    pub const RIBBON_SIMPLES: &str = "casca.ribbon.simples";
    pub const DENSIDADE_COMPACTA: &str = "casca.densidade.compacta";
    /// Prefixo das alternâncias de painel do dock: `casca.painel.<id do painel>`.
    pub const PREFIXO_PAINEL: &str = "casca.painel.";

    /// Id da Ação que mostra ou esconde um painel do dock.
    pub fn painel(id: &str) -> String {
        format!("{PREFIXO_PAINEL}{id}")
    }
}

/// Como a casca começa.
#[derive(Debug)]
pub struct ConfigCasca {
    /// Título da janela.
    pub titulo: String,
    /// Texto do `tokens.json` do app.
    pub tokens_json: String,
    /// Texto do `temas.json` do app.
    pub temas_json: String,
    pub tema_inicial: String,
    pub densidade: Densidade,
    pub escala: f32,
    pub icones: FamiliaDeIcones,
    /// Textos (pt-BR da casca + os do app, acrescentados antes).
    pub textos: Textos,
    /// Fonte da interface (Manrope no SBF), à frente da proporcional padrão do egui.
    pub fonte_interface: Option<Vec<u8>>,
}

impl ConfigCasca {
    pub fn nova(titulo: impl Into<String>, tokens_json: &str, temas_json: &str) -> Self {
        Self {
            titulo: titulo.into(),
            tokens_json: tokens_json.to_owned(),
            temas_json: temas_json.to_owned(),
            tema_inicial: String::new(),
            densidade: Densidade::Confortavel,
            escala: 1.0,
            icones: FamiliaDeIcones::sem_fonte(),
            textos: Textos::pt_br(),
            fonte_interface: None,
        }
    }
}

/// O que a janela lembra entre sessões. Serializável (JSON) para o app gravar onde quiser.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Preferencias {
    pub tema: String,
    pub densidade: Densidade,
    pub escala: f32,
    pub modo_ribbon: ModoRibbon,
    pub paineis_visiveis: BTreeMap<String, bool>,
}

/// Detecta o toque de Alt sozinho (aperta e solta sem outra tecla nem clique no meio).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct DetectorAlt {
    segurando: bool,
    sozinho: bool,
}

impl DetectorAlt {
    /// Um quadro: `alt` agora, se houve outra tecla ou clique. Devolve `true` no toque.
    pub(crate) fn quadro(&mut self, alt: bool, outra_entrada: bool) -> bool {
        let toque = match (self.segurando, alt) {
            (false, true) => {
                self.sozinho = !outra_entrada;
                false
            }
            (true, true) => {
                if outra_entrada {
                    self.sozinho = false;
                }
                false
            }
            (true, false) => self.sozinho && !outra_entrada,
            (false, false) => false,
        };
        self.segurando = alt;
        toque
    }
}

/// O estado da janela que não é do app.
#[derive(Debug)]
pub struct Estado {
    pub(crate) titulo: String,
    pub(crate) catalogo: Arc<CatalogoDeTemas>,
    pub(crate) tema: Arc<TemaAtivo>,
    pub(crate) tema_aplicado: bool,
    pub(crate) textos: Textos,
    pub(crate) icones: FamiliaDeIcones,
    pub(crate) fonte_interface: Option<Vec<u8>>,
    pub(crate) fontes_instaladas: bool,
    pub ribbon: EstadoRibbon,
    pub keytips: KeyTips,
    pub paleta: EstadoPaleta,
    pub mapa_de_atalhos_aberto: bool,
    pub historico: Historico,
    pub flutuantes: Flutuantes,
    pub(crate) paineis_visiveis: HashMap<String, bool>,
    pub(crate) dock: Option<egui_tiles::Tree<String>>,
    pub(crate) assinatura_dock: Vec<String>,
    pub(crate) alt: DetectorAlt,
    pub secao_bastidores: Option<String>,
    /// A Ação com menu cujo menu está aberto.
    pub menu_aberto: Option<IdAcao>,
    /// Larguras medidas dos grupos do Ribbon (modo simples), do quadro anterior.
    pub(crate) larguras_grupos: HashMap<String, f32>,
    /// Onde cada KeyTip é desenhado neste quadro.
    pub(crate) pontos_keytip: Vec<(String, egui::Rect)>,
    /// Erros de validação da lista de Ações ou das abas (mostrados na barra de status).
    pub erros: Vec<String>,
    pub(crate) ultimo_titulo: String,
    pub(crate) abas_do_quadro: Vec<String>,
    pub(crate) ultimas_preferencias: Option<Preferencias>,
}

impl Estado {
    /// As letras dos KeyTips desenhados no último quadro (vazio com os KeyTips desligados).
    pub fn keytips_desenhados(&self) -> Vec<&str> {
        if !self.keytips.ativo() {
            return Vec::new();
        }
        self.pontos_keytip.iter().map(|(l, _)| l.as_str()).collect()
    }

    /// Os ids das abas do app visíveis no último quadro (normais e contextuais).
    pub fn abas_visiveis(&self) -> &[String] {
        &self.abas_do_quadro
    }
}

/// Lista e abas do quadro, já com as Ações da casca.
#[derive(Debug)]
pub(crate) struct Quadro {
    pub acoes: ListaDeAcoes,
    pub abas: Vec<ribbon::Aba>,
    pub guias: Vec<ribbon::Guia>,
    pub acesso_rapido: Vec<IdAcao>,
}

/// A casca hospedando um app. Implementa `eframe::App`; nos testes, chame [`Casca::desenhar`]
/// dentro de `egui::Context::run_ui`.
#[derive(Debug)]
pub struct Casca<A: AppCasca> {
    pub app: A,
    pub estado: Estado,
}

impl<A: AppCasca> Casca<A> {
    pub fn nova(app: A, config: ConfigCasca) -> Result<Self, ErroTokens> {
        let catalogo = Arc::new(CatalogoDeTemas::ler(
            &config.tokens_json,
            &config.temas_json,
        )?);
        let tema = Arc::new(TemaAtivo::novo(
            &catalogo,
            &config.tema_inicial,
            config.densidade,
            config.escala,
        )?);
        let historico = Historico::novo(app.rota_inicial());
        let paineis_visiveis = app
            .paineis()
            .into_iter()
            .map(|p| (p.id, p.visivel_por_padrao))
            .collect();
        Ok(Self {
            app,
            estado: Estado {
                titulo: config.titulo,
                catalogo,
                tema,
                tema_aplicado: false,
                textos: config.textos,
                icones: config.icones,
                fonte_interface: config.fonte_interface,
                fontes_instaladas: false,
                ribbon: EstadoRibbon::default(),
                keytips: KeyTips::default(),
                paleta: EstadoPaleta::default(),
                mapa_de_atalhos_aberto: false,
                historico,
                flutuantes: Flutuantes::default(),
                paineis_visiveis,
                dock: None,
                assinatura_dock: Vec::new(),
                alt: DetectorAlt::default(),
                secao_bastidores: None,
                menu_aberto: None,
                larguras_grupos: HashMap::new(),
                pontos_keytip: Vec::new(),
                erros: Vec::new(),
                ultimo_titulo: String::new(),
                abas_do_quadro: Vec::new(),
                ultimas_preferencias: None,
            },
        })
    }

    /// Abre a janela com eframe e roda até fechar.
    pub fn executar(self) -> eframe::Result
    where
        A: 'static,
    {
        let titulo = self.estado.titulo.clone();
        let opcoes = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title(titulo.clone())
                .with_inner_size([1280.0, 800.0])
                .with_min_inner_size([720.0, 480.0]),
            ..Default::default()
        };
        eframe::run_native(&titulo, opcoes, Box::new(move |_cc| Ok(Box::new(self))))
    }

    /// Liga os KeyTips no nível das abas, como o toque de Alt.
    pub fn mostrar_keytips(&mut self) {
        if !self.estado.keytips.ativo() {
            self.estado.keytips.tratar(
                ribbon::EntradaKeyTip::ToqueAlt,
                &[],
                &[],
                &[],
                &ListaDeAcoes::default(),
            );
        }
    }

    pub fn tema(&self) -> &Arc<TemaAtivo> {
        &self.estado.tema
    }

    pub fn textos(&self) -> &Textos {
        &self.estado.textos
    }

    pub fn painel_visivel(&self, id: &str) -> bool {
        self.estado
            .paineis_visiveis
            .get(id)
            .copied()
            .unwrap_or(false)
    }

    /// As Ações da casca, com o estado atual.
    pub(crate) fn acoes_da_casca(&self) -> Vec<Acao> {
        let t = &self.estado.textos;
        let e = &self.estado;
        let modo = e.ribbon.modo;
        let mut acoes = vec![
            Acao::nova(ids::VOLTAR, t.t("casca-voltar"))
                .icone("voltar")
                .dica(t.t("casca-voltar-dica"))
                .atalho(Atalho::alt(Key::ArrowLeft))
                .habilitada_se(e.historico.pode_voltar(), t.t("casca-voltar-dica")),
            Acao::nova(ids::AVANCAR, t.t("casca-avancar"))
                .icone("avancar")
                .dica(t.t("casca-avancar-dica"))
                .atalho(Atalho::alt(Key::ArrowRight))
                .habilitada_se(e.historico.pode_avancar(), t.t("casca-avancar-dica")),
            Acao::nova(ids::PALETA, t.t("casca-paleta"))
                .icone("paleta")
                .dica(t.t("casca-paleta-dica"))
                .atalho(Atalho::ctrl(Key::K))
                .fora_da_paleta(),
            Acao::nova(ids::MAPA_DE_ATALHOS, t.t("casca-mapa-atalhos"))
                .icone("atalhos")
                .dica(t.t("casca-mapa-atalhos-dica"))
                .atalho(Atalho::ctrl(Key::Slash)),
            Acao::nova(ids::RIBBON_RECOLHER, t.t("casca-ribbon-recolher"))
                .icone(if modo == ModoRibbon::Recolhido {
                    "expandir"
                } else {
                    "recolher"
                })
                .dica(t.t("casca-ribbon-recolher-dica"))
                .atalho(Atalho::ctrl(Key::F1))
                .alternancia(modo == ModoRibbon::Recolhido),
            Acao::nova(ids::RIBBON_CLASSICO, t.t("casca-ribbon-classico"))
                .icone("ribbon-classico")
                .dica(t.t("casca-ribbon-classico-dica"))
                .alternancia(modo == ModoRibbon::Classico),
            Acao::nova(ids::RIBBON_SIMPLES, t.t("casca-ribbon-simples"))
                .icone("ribbon-simples")
                .dica(t.t("casca-ribbon-simples-dica"))
                .alternancia(modo == ModoRibbon::Simples),
            Acao::nova(ids::DENSIDADE_COMPACTA, t.t("casca-densidade-compacta"))
                .icone("densidade")
                .dica(t.t("casca-densidade-compacta-dica"))
                .alternancia(e.tema.densidade == Densidade::Compacta),
        ];
        for p in self.app.paineis() {
            let visivel = self.painel_visivel(&p.id);
            acoes.push(
                Acao::nova(ids::painel(&p.id), p.titulo.clone())
                    .icone(p.icone.clone())
                    .dica(t.t_com("casca-painel-dica", &[("nome", &p.titulo)]))
                    .alternancia(visivel),
            );
        }
        acoes
    }

    /// Monta a lista única do quadro (app + casca) e as abas. Erro de validação não derruba a
    /// janela: a Ação repetida sai, o atalho em conflito é retirado da segunda, e o erro vai
    /// para a barra de status.
    pub(crate) fn montar_quadro(&mut self) -> Quadro {
        let mut todas = self.app.acoes();
        todas.extend(self.acoes_da_casca());
        let mut erros = Vec::new();
        let acoes = loop {
            match ListaDeAcoes::nova(todas.clone()) {
                Ok(lista) => break lista,
                Err(erro) => {
                    erros.push(erro.to_string());
                    match erro {
                        ErroAcoes::IdVazio => todas.retain(|a| !a.id.is_empty()),
                        ErroAcoes::IdRepetido(id) => {
                            if let Some(pos) = todas.iter().rposition(|a| a.id == id) {
                                todas.remove(pos);
                            }
                        }
                        ErroAcoes::AtalhoEmConflito {
                            primeira, segunda, ..
                        } => {
                            // O atalho da casca vence; entre duas do app, fica com a primeira.
                            let perde = if segunda.starts_with("casca.") {
                                primeira
                            } else {
                                segunda
                            };
                            for a in todas.iter_mut().filter(|a| a.id == perde) {
                                a.atalho = None;
                            }
                        }
                    }
                }
            }
        };
        let abas = self.app.abas();
        if let Err(erro) = ribbon::validar_abas(&abas, &acoes) {
            erros.push(erro.to_string());
        }
        let com_arquivo = !self.app.secoes_bastidores().is_empty();
        let guias = ribbon::guias_visiveis(&abas, com_arquivo);
        let acesso_rapido = self
            .app
            .acesso_rapido()
            .into_iter()
            .filter(|id| acoes.get(id).is_some())
            .collect();
        self.estado.erros = erros;
        self.estado.abas_do_quadro = guias
            .iter()
            .filter_map(|g| match g {
                ribbon::Guia::Aba(id) => Some(id.clone()),
                ribbon::Guia::Arquivo => None,
            })
            .collect();
        Quadro {
            acoes,
            abas,
            guias,
            acesso_rapido,
        }
    }

    /// Executa uma Ação pela lista do quadro: desabilitada não roda; `casca.*` a casca trata.
    pub(crate) fn executar_acao(&mut self, ctx: &egui::Context, quadro: &Quadro, id: &str) {
        if !quadro.acoes.esta_habilitada(id) {
            return;
        }
        if quadro.acoes.get(id).is_some_and(|a| !a.menu.is_empty()) {
            // Ação com menu: abre (ou fecha) o menu; o app recebe só a Ação escolhida nele.
            let ja_aberto = self.estado.menu_aberto.as_deref() == Some(id);
            self.estado.menu_aberto = (!ja_aberto).then(|| id.to_owned());
            return;
        }
        self.estado.menu_aberto = None;
        self.estado.ribbon.apos_executar();
        if id.starts_with("casca.") {
            self.tratar_acao_da_casca(ctx, id);
        } else {
            self.app.ao_executar(ctx, id);
        }
    }

    fn tratar_acao_da_casca(&mut self, ctx: &egui::Context, id: &str) {
        let e = &mut self.estado;
        match id {
            ids::VOLTAR => pedidos::pedir(ctx, Pedido::Voltar(Local::Centro)),
            ids::AVANCAR => pedidos::pedir(ctx, Pedido::Avancar(Local::Centro)),
            ids::PALETA => e.paleta.alternar(),
            ids::MAPA_DE_ATALHOS => e.mapa_de_atalhos_aberto = !e.mapa_de_atalhos_aberto,
            ids::RIBBON_RECOLHER => e.ribbon.alternar_recolhido(),
            ids::RIBBON_CLASSICO => e.ribbon.definir_modo(ModoRibbon::Classico),
            ids::RIBBON_SIMPLES => e.ribbon.definir_modo(ModoRibbon::Simples),
            ids::DENSIDADE_COMPACTA => {
                let nova = match e.tema.densidade {
                    Densidade::Compacta => Densidade::Confortavel,
                    Densidade::Confortavel => Densidade::Compacta,
                };
                pedidos::pedir(ctx, Pedido::Densidade(nova));
            }
            outro => {
                if let Some(painel) = outro.strip_prefix(ids::PREFIXO_PAINEL) {
                    let v = e.paineis_visiveis.entry(painel.to_owned()).or_insert(false);
                    *v = !*v;
                }
            }
        }
    }

    /// As preferências atuais da janela.
    pub fn preferencias(&self) -> Preferencias {
        let t = &self.estado.tema;
        // Recolhido também é preferência (o Office lembra).
        let modo = self.estado.ribbon.modo;
        Preferencias {
            tema: t.tema.id.clone(),
            densidade: t.densidade,
            escala: t.escala,
            modo_ribbon: modo,
            paineis_visiveis: self
                .estado
                .paineis_visiveis
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect(),
        }
    }

    /// Aplica preferências gravadas. Tema desconhecido cai no primeiro do catálogo; painel que
    /// o app não tem mais é ignorado.
    pub fn aplicar_preferencias(&mut self, p: &Preferencias) {
        self.definir_aparencia(Some(&p.tema), Some(p.densidade), Some(p.escala));
        self.estado.ribbon.definir_modo(p.modo_ribbon);
        let conhecidos: Vec<String> = self.app.paineis().into_iter().map(|x| x.id).collect();
        for (id, visivel) in &p.paineis_visiveis {
            if conhecidos.contains(id) {
                self.estado.paineis_visiveis.insert(id.clone(), *visivel);
            }
        }
        self.estado.ultimas_preferencias = Some(self.preferencias());
    }

    /// Troca o Tema, a Densidade ou a Escala (o que vier `Some`).
    pub fn definir_aparencia(
        &mut self,
        tema: Option<&str>,
        densidade: Option<Densidade>,
        escala: Option<f32>,
    ) {
        let atual = &self.estado.tema;
        let tema_id = tema.unwrap_or(&atual.tema.id).to_owned();
        let densidade = densidade.unwrap_or(atual.densidade);
        let escala = escala.unwrap_or(atual.escala).clamp(0.5, 3.0);
        if let Ok(novo) = TemaAtivo::novo(&self.estado.catalogo, &tema_id, densidade, escala) {
            self.estado.tema = Arc::new(novo);
            self.estado.tema_aplicado = false;
        }
    }

    /// Atende os pedidos da fila.
    pub(crate) fn atender_pedidos(&mut self, ctx: &egui::Context, quadro: &Quadro) {
        // Um pedido pode gerar outro (Ação da casca que pede Voltar): até esvaziar, com limite.
        for _ in 0..4 {
            let fila = pedidos::drenar(ctx);
            if fila.is_empty() {
                break;
            }
            for pedido in fila {
                self.atender(ctx, quadro, pedido);
            }
        }
    }

    fn atender(&mut self, ctx: &egui::Context, quadro: &Quadro, pedido: Pedido) {
        match pedido {
            Pedido::Abrir { rota, local } => self.abrir(ctx, rota, local),
            Pedido::AbrirFlutuante(rota) => {
                let id = self.estado.flutuantes.abrir(rota.clone());
                self.app.ao_abrir_rota(ctx, &rota, Local::Flutuante(id));
            }
            Pedido::Voltar(local) => self.mover_no_historico(ctx, local, true),
            Pedido::Avancar(local) => self.mover_no_historico(ctx, local, false),
            Pedido::Executar(id) => self.executar_acao(ctx, quadro, &id),
            Pedido::Tema(id) => self.definir_aparencia(Some(&id), None, None),
            Pedido::Densidade(d) => self.definir_aparencia(None, Some(d), None),
            Pedido::Escala(e) => self.definir_aparencia(None, None, Some(e)),
            Pedido::AbrirPaleta => self.estado.paleta.abrir(),
            Pedido::FecharBastidores => self.estado.ribbon.bastidores_abertos = false,
            Pedido::AbrirAba(id) => {
                self.estado.ribbon.bastidores_abertos = false;
                self.estado.ribbon.aba_ativa = Some(id);
            }
        }
    }

    /// Abre a rota no lugar pedido. Um painel flutuante que já fechou vira o centro.
    pub fn abrir(&mut self, ctx: &egui::Context, rota: Rota, local: Local) {
        let local = match local {
            Local::Flutuante(id) => match self.estado.flutuantes.get_mut(id) {
                Some(p) => {
                    p.historico.navegar(rota.clone());
                    Local::Flutuante(id)
                }
                None => {
                    self.estado.historico.navegar(rota.clone());
                    Local::Centro
                }
            },
            Local::Centro => {
                self.estado.historico.navegar(rota.clone());
                self.estado.ribbon.bastidores_abertos = false;
                Local::Centro
            }
        };
        self.app.ao_abrir_rota(ctx, &rota, local);
    }

    fn mover_no_historico(&mut self, ctx: &egui::Context, local: Local, voltar: bool) {
        let historico = match local {
            Local::Centro => Some(&mut self.estado.historico),
            Local::Flutuante(id) => self.estado.flutuantes.get_mut(id).map(|p| &mut p.historico),
        };
        let Some(h) = historico else {
            return;
        };
        let mudou = if voltar { h.voltar() } else { h.avancar() };
        if mudou {
            let rota = h.atual().clone();
            self.app.ao_abrir_rota(ctx, &rota, local);
        }
    }

    /// Teclado e mouse globais: Alt (KeyTips), botões laterais do mouse e atalhos.
    pub(crate) fn tratar_entrada(&mut self, ctx: &egui::Context, quadro: &Quadro) {
        // Botões laterais do mouse: Voltar e Avançar no centro.
        let (voltar, avancar) = ctx.input(|i| {
            (
                i.pointer.button_pressed(PointerButton::Extra1),
                i.pointer.button_pressed(PointerButton::Extra2),
            )
        });
        if voltar {
            pedidos::pedir(ctx, Pedido::Voltar(Local::Centro));
        }
        if avancar {
            pedidos::pedir(ctx, Pedido::Avancar(Local::Centro));
        }

        // Toque de Alt.
        let (alt, outra) = ctx.input(|i| {
            let outra = i.events.iter().any(|e| {
                matches!(
                    e,
                    egui::Event::Key { pressed: true, .. }
                        | egui::Event::PointerButton { pressed: true, .. }
                )
            });
            (i.modifiers.alt, outra)
        });
        let modal_aberto = self.estado.paleta.aberta || self.estado.mapa_de_atalhos_aberto;
        if self.estado.alt.quadro(alt, outra) && !modal_aberto {
            let r = self.estado.keytips.tratar(
                ribbon::EntradaKeyTip::ToqueAlt,
                &quadro.abas,
                &quadro.guias,
                &quadro.acesso_rapido,
                &quadro.acoes,
            );
            self.aplicar_keytip(ctx, quadro, r);
        }

        // KeyTips ativos: as letras são deles.
        if self.estado.keytips.ativo() {
            let teclas: Vec<Key> = ctx.input_mut(|i| {
                let mut teclas = Vec::new();
                i.events.retain(|e| match e {
                    egui::Event::Key {
                        key, pressed: true, ..
                    } if letra_de(*key).is_some() || *key == Key::Escape => {
                        teclas.push(*key);
                        false
                    }
                    egui::Event::Text(_) => false,
                    _ => true,
                });
                teclas
            });
            for tecla in teclas {
                let entrada = match tecla {
                    Key::Escape => ribbon::EntradaKeyTip::Esc,
                    outra => match letra_de(outra) {
                        Some(c) => ribbon::EntradaKeyTip::Letra(c),
                        None => continue,
                    },
                };
                let r = self.estado.keytips.tratar(
                    entrada,
                    &quadro.abas,
                    &quadro.guias,
                    &quadro.acesso_rapido,
                    &quadro.acoes,
                );
                self.aplicar_keytip(ctx, quadro, r);
            }
            // Clique em qualquer lugar fecha os KeyTips.
            if ctx.input(|i| i.pointer.any_pressed()) {
                self.estado.keytips.fechar();
            }
            return;
        }

        // Esc fecha os bastidores.
        if self.estado.ribbon.bastidores_abertos
            && !modal_aberto
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape))
        {
            self.estado.ribbon.bastidores_abertos = false;
        }

        // Atalhos: modificadores exatos; com um campo de texto em foco, só os com Ctrl ou Alt.
        let digitando = ctx.egui_wants_keyboard_input();
        let disparadas: Vec<IdAcao> = ctx.input_mut(|i| {
            let mut ids = Vec::new();
            i.events.retain(|e| {
                let egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = e
                else {
                    return true;
                };
                let Some(acao) = quadro.acoes.por_atalho(*key, *modifiers) else {
                    return true;
                };
                let forte = acao.atalho.is_some_and(|a| a.tem_modificador_forte());
                if digitando && !forte {
                    return true;
                }
                // Com a Paleta aberta só o próprio Ctrl+K (fecha) passa.
                if modal_aberto && acao.id != ids::PALETA && acao.id != ids::MAPA_DE_ATALHOS {
                    return true;
                }
                ids.push(acao.id.clone());
                false
            });
            ids
        });
        for id in disparadas {
            self.executar_acao(ctx, quadro, &id);
        }
    }

    fn aplicar_keytip(&mut self, ctx: &egui::Context, quadro: &Quadro, r: ResultadoKeyTip) {
        match r {
            ResultadoKeyTip::Nada => {}
            ResultadoKeyTip::AbrirAba(id) => {
                let guia = ribbon::Guia::Aba(id.clone());
                let e = &mut self.estado.ribbon;
                e.bastidores_abertos = false;
                e.aba_ativa = Some(id);
                if e.modo == ModoRibbon::Recolhido {
                    e.corpo_aberto = true;
                } else {
                    e.clicar_guia(&guia, false);
                }
            }
            ResultadoKeyTip::AbrirBastidores => {
                self.estado
                    .ribbon
                    .clicar_guia(&ribbon::Guia::Arquivo, false);
            }
            ResultadoKeyTip::Executar(id) => self.executar_acao(ctx, quadro, &id),
        }
    }

    /// Um quadro inteiro, dentro da `Ui` raiz da janela.
    pub fn desenhar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.preparar_contexto(&ctx);
        let quadro = self.montar_quadro();
        self.tratar_entrada(&ctx, &quadro);
        self.estado.pontos_keytip.clear();
        crate::vista::desenhar_janela(self, ui, &quadro);
        self.atender_pedidos(&ctx, &quadro);
        let agora = self.preferencias();
        match &self.estado.ultimas_preferencias {
            None => self.estado.ultimas_preferencias = Some(agora),
            Some(antes) if *antes != agora => {
                self.app.ao_mudar_preferencias(&agora);
                self.estado.ultimas_preferencias = Some(agora);
            }
            Some(_) => {}
        }
        let titulo = format!(
            "{} — {}",
            self.app.titulo_da_rota(self.estado.historico.atual()),
            self.estado.titulo
        );
        if titulo != self.estado.ultimo_titulo {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(titulo.clone()));
            self.estado.ultimo_titulo = titulo;
        }
    }

    fn preparar_contexto(&mut self, ctx: &egui::Context) {
        if !self.estado.fontes_instaladas {
            let mut fontes = egui::FontDefinitions::default();
            if let Some(bytes) = &self.estado.fonte_interface {
                fontes.font_data.insert(
                    "bancada-interface".to_owned(),
                    Arc::new(egui::FontData::from_owned(bytes.clone())),
                );
                fontes
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .insert(0, "bancada-interface".to_owned());
            }
            self.estado.icones.instalar(&mut fontes);
            ctx.set_fonts(fontes);
            self.estado.fontes_instaladas = true;
        }
        if !self.estado.tema_aplicado {
            self.estado.tema.aplicar(ctx);
            ctx.data_mut(|d| {
                d.insert_temp(
                    egui::Id::new("bancada-casca.catalogo"),
                    Arc::clone(&self.estado.catalogo),
                );
            });
            self.estado.tema_aplicado = true;
        }
    }
}

/// O catálogo de Temas guardado no `Context` (para a seção de Opções do app).
pub fn catalogo_do_contexto(ctx: &egui::Context) -> Option<Arc<CatalogoDeTemas>> {
    ctx.data(|d| d.get_temp::<Arc<CatalogoDeTemas>>(egui::Id::new("bancada-casca.catalogo")))
}

/// A letra de uma tecla para os KeyTips (`A`–`Z`, `0`–`9`).
pub(crate) fn letra_de(tecla: Key) -> Option<char> {
    let nome = tecla.name();
    let mut chars = nome.chars();
    let c = chars.next()?;
    (chars.next().is_none() && c.is_ascii_alphanumeric()).then_some(c.to_ascii_uppercase())
}

impl<A: AppCasca> eframe::App for Casca<A> {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.desenhar(ui);
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn toque_de_alt_sozinho() {
        let mut d = DetectorAlt::default();
        assert!(!d.quadro(true, false));
        assert!(d.quadro(false, false), "aperta e solta: toque");
        assert!(!d.quadro(true, false));
        assert!(!d.quadro(true, true), "Alt+← não é toque");
        assert!(!d.quadro(false, false));
        assert!(!d.quadro(false, false));
    }

    #[test]
    fn letras_dos_keytips() {
        assert_eq!(letra_de(Key::T), Some('T'));
        assert_eq!(letra_de(Key::Num1), Some('1'));
        assert_eq!(letra_de(Key::F1), None);
        assert_eq!(letra_de(Key::Escape), None);
    }
}
