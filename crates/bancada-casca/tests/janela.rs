//! Testes da janela sem janela: `egui::Context::run_ui` com entrada sintética. Conferem que o
//! desenho não entra em pânico em nenhum Tema, Densidade ou modo, e que teclado, mouse e
//! pedidos chegam onde devem.

mod comum;

use bancada_casca::egui::{self, Event, Key, Modifiers, PointerButton, RawInput, Rect, pos2, vec2};
use bancada_casca::tema::CatalogoDeTemas;
use bancada_casca::{Casca, Densidade, Local, ModoRibbon, Pedido, Rota, TemaAtivo, ids, pedir};
use comum::app_exemplo::{AppExemplo, TEMAS, TOKENS};

fn casca() -> Casca<AppExemplo> {
    Casca::nova(AppExemplo::novo(), AppExemplo::config()).expect("tokens do exemplo")
}

fn entrada(eventos: Vec<Event>, mods: Modifiers) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0))),
        events: std::iter::once(Event::ModifiersChanged(mods))
            .chain(eventos)
            .collect(),
        ..Default::default()
    }
}

/// Um quadro com eventos e modificadores.
fn quadro(c: &mut Casca<AppExemplo>, ctx: &egui::Context, eventos: Vec<Event>, mods: Modifiers) {
    ctx.run_ui(entrada(eventos, mods), |ui| c.desenhar(ui))
        .drop_without_applying_deltas();
}

fn vazio(c: &mut Casca<AppExemplo>, ctx: &egui::Context) {
    quadro(c, ctx, Vec::new(), Modifiers::NONE);
}

fn tecla(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn apertar(c: &mut Casca<AppExemplo>, ctx: &egui::Context, key: Key, mods: Modifiers) {
    quadro(c, ctx, vec![tecla(key, mods)], mods);
}

#[test]
fn os_sete_temas_do_sbf_carregam_do_temas_json_real() {
    let cat = CatalogoDeTemas::ler(TOKENS, TEMAS).expect("tokens.json e temas.json do SBF");
    let ids: Vec<&str> = cat.temas().iter().map(|t| t.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            "salvia", "ocre", "azul", "linho", "menta", "noite", "grafite"
        ]
    );
    for t in cat.temas() {
        let ativo = TemaAtivo::novo(&cat, &t.id, Densidade::Confortavel, 1.0)
            .unwrap_or_else(|e| panic!("{}: {e}", t.id));
        assert_eq!(ativo.tema.id, t.id);
    }
    let salvia = TemaAtivo::novo(&cat, "salvia", Densidade::Confortavel, 1.0).unwrap();
    let noite = TemaAtivo::novo(&cat, "noite", Densidade::Confortavel, 1.0).unwrap();
    // `ink-muted` = `--text-3` do SBF, já corrigido na rodada de contraste.
    assert_eq!(
        salvia.paleta.tinta_suave,
        egui::Color32::from_rgb(0x61, 0x6d, 0x68)
    );
    assert_ne!(salvia.paleta.superficie_0, noite.paleta.superficie_0);
    let ocre = TemaAtivo::novo(&cat, "ocre", Densidade::Confortavel, 1.0).unwrap();
    assert_eq!(
        ocre.paleta.superficie_0,
        egui::Color32::from_rgb(0xf7, 0xf2, 0xe8)
    );
    // Token que o ocre não troca vem da base clara (sálvia).
    assert_eq!(
        ocre.cor("on-accent"),
        Some(egui::Color32::from_rgb(0xff, 0xfd, 0xf7))
    );
    assert!(salvia.cor("hero-bg").is_some());
    // Densidade compacta encolhe os controles.
    let compacta = TemaAtivo::novo(&cat, "salvia", Densidade::Compacta, 1.0).unwrap();
    assert!(compacta.medidas.altura_controle < salvia.medidas.altura_controle);
}

#[test]
fn desenha_em_todos_os_temas_densidades_escalas_e_modos_sem_panico() {
    let mut c = casca();
    let ctx = egui::Context::default();
    let temas: Vec<String> = CatalogoDeTemas::ler(TOKENS, TEMAS)
        .unwrap()
        .temas()
        .iter()
        .map(|t| t.id.clone())
        .collect();
    for tema in &temas {
        for densidade in [Densidade::Confortavel, Densidade::Compacta] {
            for modo in [
                ModoRibbon::Classico,
                ModoRibbon::Simples,
                ModoRibbon::Recolhido,
            ] {
                c.definir_aparencia(Some(tema), Some(densidade), Some(1.25));
                c.estado.ribbon.definir_modo(modo);
                vazio(&mut c, &ctx);
                vazio(&mut c, &ctx);
                let ativo = TemaAtivo::do_contexto(&ctx).expect("tema no contexto");
                assert_eq!(&ativo.tema.id, tema);
                assert_eq!(ativo.densidade, densidade);
            }
        }
    }
    assert!((ctx.zoom_factor() - 1.25).abs() < 1e-6, "escala vira zoom");
    assert!(c.estado.erros.is_empty(), "{:?}", c.estado.erros);
}

#[test]
fn ctrl_k_abre_e_fecha_a_paleta_e_esc_fecha() {
    let mut c = casca();
    let ctx = egui::Context::default();
    vazio(&mut c, &ctx);
    apertar(&mut c, &ctx, Key::K, Modifiers::COMMAND);
    assert!(c.estado.paleta.aberta);
    vazio(&mut c, &ctx);
    apertar(&mut c, &ctx, Key::K, Modifiers::COMMAND);
    assert!(!c.estado.paleta.aberta);
    apertar(&mut c, &ctx, Key::K, Modifiers::COMMAND);
    vazio(&mut c, &ctx);
    apertar(&mut c, &ctx, Key::Escape, Modifiers::NONE);
    vazio(&mut c, &ctx);
    assert!(!c.estado.paleta.aberta);
}

#[test]
fn paleta_busca_e_enter_abre_o_destino() {
    let mut c = casca();
    let ctx = egui::Context::default();
    vazio(&mut c, &ctx);
    apertar(&mut c, &ctx, Key::K, Modifiers::COMMAND);
    c.estado.paleta.busca = "bicicl".into();
    vazio(&mut c, &ctx);
    apertar(&mut c, &ctx, Key::Enter, Modifiers::NONE);
    vazio(&mut c, &ctx);
    assert_eq!(c.estado.historico.atual(), &Rota::nova("projeto:oficina"));
    assert!(!c.estado.paleta.aberta);
}

#[test]
fn paleta_executa_acao_do_app() {
    let mut c = casca();
    let ctx = egui::Context::default();
    vazio(&mut c, &ctx);
    apertar(&mut c, &ctx, Key::K, Modifiers::COMMAND);
    c.estado.paleta.busca = "novo item".into();
    vazio(&mut c, &ctx);
    apertar(&mut c, &ctx, Key::Enter, Modifiers::NONE);
    vazio(&mut c, &ctx);
    assert_eq!(c.app.executadas, vec!["item.novo".to_owned()]);
}

#[test]
fn historico_por_alt_setas_e_botoes_laterais_do_mouse() {
    let mut c = casca();
    let ctx = egui::Context::default();
    vazio(&mut c, &ctx);
    pedir(
        &ctx,
        Pedido::Abrir {
            rota: "projetos".into(),
            local: Local::Centro,
        },
    );
    vazio(&mut c, &ctx);
    pedir(
        &ctx,
        Pedido::Abrir {
            rota: "projeto:jardim".into(),
            local: Local::Centro,
        },
    );
    vazio(&mut c, &ctx);
    assert_eq!(c.estado.historico.atual(), &Rota::nova("projeto:jardim"));
    apertar(&mut c, &ctx, Key::ArrowLeft, Modifiers::ALT);
    vazio(&mut c, &ctx);
    assert_eq!(c.estado.historico.atual(), &Rota::nova("projetos"));
    apertar(&mut c, &ctx, Key::ArrowRight, Modifiers::ALT);
    vazio(&mut c, &ctx);
    assert_eq!(c.estado.historico.atual(), &Rota::nova("projeto:jardim"));
    // Botão lateral "voltar" do mouse (Extra1).
    let clique = |pressed| Event::PointerButton {
        pos: pos2(600.0, 400.0),
        button: PointerButton::Extra1,
        pressed,
        modifiers: Modifiers::NONE,
    };
    quadro(
        &mut c,
        &ctx,
        vec![Event::PointerMoved(pos2(600.0, 400.0)), clique(true)],
        Modifiers::NONE,
    );
    quadro(&mut c, &ctx, vec![clique(false)], Modifiers::NONE);
    vazio(&mut c, &ctx);
    assert_eq!(c.estado.historico.atual(), &Rota::nova("projetos"));
    // O app foi avisado de cada rota aberta.
    let ultimas: Vec<&str> = c
        .app
        .rotas_abertas
        .iter()
        .map(|(r, _)| r.como_texto())
        .collect();
    assert_eq!(
        ultimas,
        vec![
            "projetos",
            "projeto:jardim",
            "projetos",
            "projeto:jardim",
            "projetos"
        ]
    );
    // Alt+← sozinho não liga os KeyTips.
    assert!(!c.estado.keytips.ativo());
}

#[test]
fn ctrl_clique_abre_paineis_flutuantes_que_navegam_e_se_destacam() {
    let mut c = casca();
    let ctx = egui::Context::default();
    vazio(&mut c, &ctx);
    pedir(&ctx, Pedido::AbrirFlutuante("projeto:jardim".into()));
    pedir(&ctx, Pedido::AbrirFlutuante("contato:ana".into()));
    vazio(&mut c, &ctx);
    assert_eq!(c.estado.flutuantes.len(), 2);
    let ids = c.estado.flutuantes.ids();
    pedir(
        &ctx,
        Pedido::Abrir {
            rota: "tarefa:t1".into(),
            local: Local::Flutuante(ids[0]),
        },
    );
    vazio(&mut c, &ctx);
    let p0 = c.estado.flutuantes.get(ids[0]).unwrap();
    assert_eq!(p0.historico.atual(), &Rota::nova("tarefa:t1"));
    assert_eq!(
        c.estado.historico.atual(),
        &Rota::nova("inicio"),
        "o centro não mudou"
    );
    pedir(&ctx, Pedido::Voltar(Local::Flutuante(ids[0])));
    vazio(&mut c, &ctx);
    assert_eq!(
        c.estado.flutuantes.get(ids[0]).unwrap().historico.atual(),
        &Rota::nova("projeto:jardim")
    );
    // Destacado: sem suporte a várias janelas (teste), vira janela embutida, sem pânico.
    c.estado.flutuantes.destacar(ids[1], true);
    vazio(&mut c, &ctx);
    vazio(&mut c, &ctx);
    assert!(c.estado.flutuantes.get(ids[1]).unwrap().destacado);
    // Pedido para um flutuante que fechou cai no centro.
    c.estado.flutuantes.fechar(ids[0]);
    pedir(
        &ctx,
        Pedido::Abrir {
            rota: "contatos".into(),
            local: Local::Flutuante(ids[0]),
        },
    );
    vazio(&mut c, &ctx);
    assert_eq!(c.estado.historico.atual(), &Rota::nova("contatos"));
}

/// Aperta e solta o Alt sozinho.
fn toque_alt(c: &mut Casca<AppExemplo>, ctx: &egui::Context) {
    quadro(c, ctx, Vec::new(), Modifiers::ALT);
    vazio(c, ctx);
}

#[test]
fn keytips_alt_aba_e_acao_pelo_teclado() {
    let mut c = casca();
    let ctx = egui::Context::default();
    vazio(&mut c, &ctx);
    toque_alt(&mut c, &ctx);
    assert!(c.estado.keytips.ativo());
    vazio(&mut c, &ctx);
    apertar(&mut c, &ctx, Key::I, Modifiers::NONE);
    assert_eq!(c.estado.ribbon.aba_ativa.as_deref(), Some("inicio"));
    vazio(&mut c, &ctx);
    apertar(&mut c, &ctx, Key::N, Modifiers::NONE);
    assert_eq!(c.app.executadas, vec!["item.novo".to_owned()]);
    assert!(!c.estado.keytips.ativo());
    // Desabilitada (sem seleção) não dispara.
    toque_alt(&mut c, &ctx);
    apertar(&mut c, &ctx, Key::T, Modifiers::NONE);
    apertar(&mut c, &ctx, Key::X, Modifiers::NONE);
    assert_eq!(c.app.executadas.len(), 1);
    apertar(&mut c, &ctx, Key::Escape, Modifiers::NONE);
    apertar(&mut c, &ctx, Key::Escape, Modifiers::NONE);
    assert!(!c.estado.keytips.ativo());
    // A aba Arquivo pelo KeyTip abre os bastidores.
    toque_alt(&mut c, &ctx);
    apertar(&mut c, &ctx, Key::A, Modifiers::NONE);
    assert!(c.estado.ribbon.bastidores_abertos);
    vazio(&mut c, &ctx);
    apertar(&mut c, &ctx, Key::Escape, Modifiers::NONE);
    assert!(!c.estado.ribbon.bastidores_abertos);
    // Números: Barra de Acesso Rápido (2 = Paleta).
    toque_alt(&mut c, &ctx);
    apertar(&mut c, &ctx, Key::Num2, Modifiers::NONE);
    assert!(c.estado.paleta.aberta);
}

#[test]
fn keytips_desenham_os_selos_sobre_abas_e_acoes() {
    let mut c = casca();
    let ctx = egui::Context::default();
    vazio(&mut c, &ctx);
    toque_alt(&mut c, &ctx);
    vazio(&mut c, &ctx);
    let letras: Vec<&str> = c.estado.keytips_desenhados();
    assert!(
        letras.contains(&"A") && letras.contains(&"I") && letras.contains(&"T"),
        "{letras:?}"
    );
    assert!(letras.contains(&"1"), "Barra de Acesso Rápido numerada");
    apertar(&mut c, &ctx, Key::T, Modifiers::NONE);
    vazio(&mut c, &ctx);
    let letras: Vec<&str> = c.estado.keytips_desenhados();
    assert!(
        letras.contains(&"UD") && letras.contains(&"X"),
        "{letras:?}"
    );
}

#[test]
fn aba_contextual_aparece_com_selecao_e_some_sem() {
    let mut c = casca();
    let ctx = egui::Context::default();
    vazio(&mut c, &ctx);
    assert!(!c.estado.abas_visiveis().contains(&"item".to_owned()));
    c.app.selecionado = Some("jardim".into());
    vazio(&mut c, &ctx);
    assert!(c.estado.abas_visiveis().contains(&"item".to_owned()));
    c.estado.ribbon.aba_ativa = Some("item".into());
    vazio(&mut c, &ctx);
    c.app.selecionado = None;
    vazio(&mut c, &ctx);
    vazio(&mut c, &ctx);
    assert!(!c.estado.abas_visiveis().contains(&"item".to_owned()));
}

#[test]
fn ctrl_f1_e_atalhos_do_app_e_acao_desabilitada_nao_roda() {
    let mut c = casca();
    let ctx = egui::Context::default();
    vazio(&mut c, &ctx);
    apertar(&mut c, &ctx, Key::F1, Modifiers::COMMAND);
    assert_eq!(c.estado.ribbon.modo, ModoRibbon::Recolhido);
    apertar(&mut c, &ctx, Key::F1, Modifiers::COMMAND);
    assert_eq!(c.estado.ribbon.modo, ModoRibbon::Classico);
    apertar(&mut c, &ctx, Key::F5, Modifiers::NONE);
    apertar(&mut c, &ctx, Key::Delete, Modifiers::NONE);
    assert_eq!(c.app.executadas, vec!["geral.atualizar".to_owned()]);
    c.app.selecionado = Some("jardim".into());
    vazio(&mut c, &ctx);
    apertar(&mut c, &ctx, Key::Delete, Modifiers::NONE);
    assert_eq!(
        c.app.executadas.last().map(String::as_str),
        Some("item.excluir")
    );
    // Ctrl+/ abre o Mapa de atalhos.
    apertar(&mut c, &ctx, Key::Slash, Modifiers::COMMAND);
    vazio(&mut c, &ctx);
    assert!(c.estado.mapa_de_atalhos_aberto);
}

#[test]
fn caixas_dos_paineis_do_dock_mostram_e_escondem() {
    let mut c = casca();
    let ctx = egui::Context::default();
    vazio(&mut c, &ctx);
    assert!(c.painel_visivel("detalhe"));
    pedir(&ctx, Pedido::Executar(ids::painel("detalhe")));
    vazio(&mut c, &ctx);
    assert!(!c.painel_visivel("detalhe"));
    for p in ["origens", "historico"] {
        pedir(&ctx, Pedido::Executar(ids::painel(p)));
    }
    vazio(&mut c, &ctx);
    vazio(&mut c, &ctx);
    assert!(!c.painel_visivel("origens") && !c.painel_visivel("historico"));
    pedir(&ctx, Pedido::Executar(ids::painel("historico")));
    vazio(&mut c, &ctx);
    assert!(c.painel_visivel("historico"));
}

#[test]
fn bastidores_paleta_mapa_e_tudo_aberto_ao_mesmo_tempo_sem_panico() {
    let mut c = casca();
    let ctx = egui::Context::default();
    c.estado.ribbon.bastidores_abertos = true;
    for _ in 0..3 {
        vazio(&mut c, &ctx);
    }
    c.estado.secao_bastidores = Some("opcoes".into());
    vazio(&mut c, &ctx);
    pedir(&ctx, Pedido::Tema("grafite".into()));
    vazio(&mut c, &ctx);
    vazio(&mut c, &ctx);
    assert_eq!(TemaAtivo::do_contexto(&ctx).unwrap().tema.id, "grafite");
    c.estado.ribbon.bastidores_abertos = false;
    c.estado.paleta.abrir();
    c.estado.mapa_de_atalhos_aberto = true;
    pedir(&ctx, Pedido::AbrirFlutuante("projeto:oficina".into()));
    c.estado.ribbon.definir_modo(ModoRibbon::Recolhido);
    c.estado.ribbon.corpo_aberto = true;
    for _ in 0..3 {
        vazio(&mut c, &ctx);
    }
}

/// App com Ações em conflito: a janela abre, e o erro vai para a barra de status.
#[test]
fn lista_invalida_do_app_nao_derruba_a_janela() {
    struct Ruim(AppExemplo);
    impl bancada_casca::AppCasca for Ruim {
        fn acoes(&self) -> Vec<bancada_casca::Acao> {
            let mut a = self.0.acoes();
            a.push(bancada_casca::Acao::nova("item.novo", "Repetida"));
            a.push(
                bancada_casca::Acao::nova("conflito", "Conflito")
                    .atalho(bancada_casca::Atalho::ctrl(Key::K)),
            );
            a
        }
        fn abas(&self) -> Vec<bancada_casca::Aba> {
            self.0.abas()
        }
        fn navegacao(&self) -> Vec<bancada_casca::GrupoNavegacao> {
            self.0.navegacao()
        }
        fn rota_inicial(&self) -> Rota {
            self.0.rota_inicial()
        }
        fn trilha(&self, rota: &Rota) -> Vec<bancada_casca::Elo> {
            self.0.trilha(rota)
        }
        fn desenhar_pagina(&mut self, ui: &mut egui::Ui, rota: &Rota, local: Local) {
            self.0.desenhar_pagina(ui, rota, local);
        }
        fn ao_executar(&mut self, ctx: &egui::Context, id: &str) {
            self.0.ao_executar(ctx, id);
        }
        fn paineis(&self) -> Vec<bancada_casca::PainelDock> {
            self.0.paineis()
        }
    }
    let mut c = Casca::nova(Ruim(AppExemplo::novo()), AppExemplo::config()).unwrap();
    let ctx = egui::Context::default();
    ctx.run_ui(entrada(Vec::new(), Modifiers::NONE), |ui| c.desenhar(ui))
        .drop_without_applying_deltas();
    assert_eq!(c.estado.erros.len(), 2, "{:?}", c.estado.erros);
    // O Ctrl+K continua da Paleta.
    ctx.run_ui(
        entrada(vec![tecla(Key::K, Modifiers::COMMAND)], Modifiers::COMMAND),
        |ui| c.desenhar(ui),
    )
    .drop_without_applying_deltas();
    assert!(c.estado.paleta.aberta);
    assert!(c.app.0.executadas.is_empty());
}

#[test]
fn barra_de_status_fica_inteira_no_rodape() {
    let mut c = casca();
    let ctx = egui::Context::default();
    vazio(&mut c, &ctx);
    vazio(&mut c, &ctx);
    let status = egui::containers::panel::PanelState::load(&ctx, egui::Id::new("casca.status"))
        .expect("painel de status");
    let ribbon = egui::containers::panel::PanelState::load(&ctx, egui::Id::new("casca.ribbon"))
        .expect("ribbon");
    assert!((status.outer_rect.bottom() - 800.0).abs() < 0.5);
    assert!(
        status.outer_rect.height() >= 23.5,
        "{:?}",
        status.outer_rect
    );
    // O Ribbon clássico tem a altura dos tokens: abas (32) + corpo (96) + linhas.
    assert!(
        ribbon.outer_rect.height() <= 32.0 + 96.0 + 4.0,
        "{:?}",
        ribbon.outer_rect
    );
}

#[test]
fn acao_com_menu_abre_o_menu_e_o_app_recebe_so_a_escolha() {
    let mut c = casca();
    let ctx = egui::Context::default();
    vazio(&mut c, &ctx);
    pedir(&ctx, Pedido::Executar("filtro.estado".into()));
    vazio(&mut c, &ctx);
    vazio(&mut c, &ctx);
    assert_eq!(c.estado.menu_aberto.as_deref(), Some("filtro.estado"));
    assert!(c.app.executadas.is_empty(), "abrir o menu não chega ao app");
    pedir(&ctx, Pedido::Executar("filtro.abertos".into()));
    vazio(&mut c, &ctx);
    assert_eq!(c.estado.menu_aberto, None);
    assert_eq!(c.app.filtro, "abertos");
    // De novo, e Esc fecha.
    pedir(&ctx, Pedido::Executar("filtro.estado".into()));
    vazio(&mut c, &ctx);
    vazio(&mut c, &ctx);
    apertar(&mut c, &ctx, Key::Escape, Modifiers::NONE);
    vazio(&mut c, &ctx);
    assert_eq!(c.estado.menu_aberto, None);
}

#[test]
fn preferencias_vao_e_voltam_em_json_e_o_app_e_avisado() {
    let mut c = casca();
    let ctx = egui::Context::default();
    vazio(&mut c, &ctx);
    pedir(&ctx, Pedido::Tema("menta".into()));
    pedir(&ctx, Pedido::Densidade(Densidade::Compacta));
    pedir(&ctx, Pedido::Executar(ids::painel("origens")));
    vazio(&mut c, &ctx);
    vazio(&mut c, &ctx);
    apertar(&mut c, &ctx, Key::F1, Modifiers::COMMAND);
    vazio(&mut c, &ctx);
    let p = c.preferencias();
    assert_eq!(p.tema, "menta");
    assert_eq!(p.densidade, Densidade::Compacta);
    assert_eq!(p.modo_ribbon, ModoRibbon::Recolhido);
    assert_eq!(p.paineis_visiveis.get("origens"), Some(&false));
    let json = serde_json::to_string(&p).unwrap();
    let mut outra = casca();
    outra.aplicar_preferencias(&serde_json::from_str(&json).unwrap());
    assert_eq!(outra.preferencias(), p);
    assert!(!c.app.preferencias_gravadas.is_empty(), "o app foi avisado");
    assert_eq!(c.app.preferencias_gravadas.last(), Some(&p));
}

#[test]
fn abas_da_pagina_comecam_na_primeira_disponivel_e_ignoram_a_desabilitada() {
    use bancada_casca::vista::{AbaDePagina, aba_escolhida};
    let ctx = egui::Context::default();
    let rota = Rota::nova("projeto:jardim");
    let abas = [
        AbaDePagina::nova("movimentacoes", "Movimentações").indisponivel("Chega com o livro-caixa"),
        AbaDePagina::nova("registros", "Registros · a conferir").contagem(3),
    ];
    assert_eq!(
        aba_escolhida(&ctx, &rota, &abas).as_deref(),
        Some("registros")
    );
    // E a página desenha com elas, sem pânico, e o pedido de aba do Ribbon funciona.
    let mut c = casca();
    pedir(
        &ctx,
        Pedido::Abrir {
            rota: rota.clone(),
            local: Local::Centro,
        },
    );
    vazio(&mut c, &ctx);
    pedir(&ctx, Pedido::AbrirAba("exibir".into()));
    vazio(&mut c, &ctx);
    vazio(&mut c, &ctx);
    assert_eq!(c.estado.ribbon.aba_ativa.as_deref(), Some("exibir"));
}

/// Com uma fonte de ícones própria (aqui a Ubuntu-Light do egui, só para o teste), a família
/// nomeada existe e o desenho não entra em pânico; e uma fonte que o app soma depois com
/// `add_font` não apaga a dos ícones.
#[test]
fn fonte_de_icones_do_app_e_registrada_sem_apagar_as_outras() {
    let bytes = egui::FontDefinitions::default()
        .font_data
        .get("Ubuntu-Light")
        .map(|d| d.font.to_vec())
        .expect("fonte padrão do egui");
    let mut config = AppExemplo::config();
    config.icones = bancada_casca::FamiliaDeIcones::com_fonte(bytes.clone())
        .glifo("projeto", "P")
        .glifo("novo", "+");
    let mut c = Casca::nova(AppExemplo::novo(), config).unwrap();
    let ctx = egui::Context::default();
    vazio(&mut c, &ctx);
    ctx.add_font(egui::epaint::text::FontInsert::new(
        "fonte-do-app",
        egui::FontData::from_owned(bytes),
        vec![egui::epaint::text::InsertFontFamily {
            family: egui::FontFamily::Name("serifa".into()),
            priority: egui::epaint::text::FontPriority::Highest,
        }],
    ));
    vazio(&mut c, &ctx);
    vazio(&mut c, &ctx);
    let familias = ctx.fonts(|f| f.definitions().families.keys().cloned().collect::<Vec<_>>());
    assert!(familias.contains(&egui::FontFamily::Name(
        bancada_casca::icones::FAMILIA_ICONES.into()
    )));
    assert!(familias.contains(&egui::FontFamily::Name("serifa".into())));
}

#[test]
fn alt_mais_letra_vai_direto_para_a_aba() {
    let mut c = casca();
    let ctx = egui::Context::default();
    vazio(&mut c, &ctx);
    apertar(&mut c, &ctx, Key::T, Modifiers::ALT);
    vazio(&mut c, &ctx);
    assert_eq!(c.estado.ribbon.aba_ativa.as_deref(), Some("triagem"));
    assert!(c.estado.keytips.ativo(), "fica no nível das Ações da aba");
    // Soltar o Alt depois de Alt+T não é toque: os KeyTips continuam.
    vazio(&mut c, &ctx);
    assert!(c.estado.keytips.ativo());
    apertar(&mut c, &ctx, Key::Escape, Modifiers::NONE);
    apertar(&mut c, &ctx, Key::Escape, Modifiers::NONE);
    assert!(!c.estado.keytips.ativo());
    // Alt+← continua sendo Voltar, não KeyTip.
    apertar(&mut c, &ctx, Key::ArrowLeft, Modifiers::ALT);
    assert!(!c.estado.keytips.ativo());
}
