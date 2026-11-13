# bancada-casca

A shell desktop compartilhada pelo Seu Bolso Feliz e pelo Chess Studio (ADR-018 do SBF), em
**egui 0.36 / eframe 0.36 puro** (renderizador glow), sem Bevy e sem domínio. Extraída e
portada do Chess Studio (`ribbon/`, `palette.rs`, `ui_theme/`, `i18n.rs`, `chess-input`
`actions.rs`/`controls.rs`), com identificadores em português.

```
┌ barra de título: Barra de Acesso Rápido · título da página ─────────────────────┐
│ Ribbon: Arquivo | abas do app | abas contextuais                                │
│   corpo clássico / simples / recolhido (Ctrl+F1) · KeyTips (Alt)                 │
├ navegação ─┬ ◀ ▶ trilha › ... ───────────────────────────────┬ dock (egui_tiles)┤
│ grupos     │ centro: o app desenha a página da rota           │ painéis do app   │
│ itens      │ (link_item em todo nome; Ctrl+clique = flutuante)│                  │
├────────────┴──────────────────────────────────────────────────┴──────────────────┤
│ barra de status: segmentos do app                                               │
└──────────────────────────────────────────────────────────────────────────────────┘
 por cima: Paleta (Ctrl+K) · Mapa de atalhos (Ctrl+/) · painéis flutuantes (destacáveis
 em janela do sistema) · bastidores (aba Arquivo)
```

## Como o app se liga

O app é um `impl AppCasca`. A casca **pergunta** a cada quadro (Ações, abas, navegação,
painéis, status, bastidores) e **avisa** (`ao_executar`, `ao_abrir_rota`,
`ao_clicar_status`). De dentro do desenho, o app **pede** à casca só com o `egui::Context`:
`link_item`, `abrir`, `abrir_flutuante`, `executar` e `pedir(ctx, Pedido::…)`.

```rust
use bancada_casca::{
    Aba, Acao, AppCasca, Atalho, Casca, ConfigCasca, Elo, Grupo, GrupoNavegacao,
    ItemNavegacao, Local, Rota, egui, link_item,
};

struct MeuApp;

impl AppCasca for MeuApp {
    fn acoes(&self) -> Vec<Acao> {
        vec![
            Acao::nova("triagem.descartar", "Descartar")
                .icone("delete")
                .keytip("X")
                .atalho(Atalho::tecla(egui::Key::Delete)),
            Acao::nova("lote.enviar", "Enviar ao servidor")
                .desabilitada("Chega na fatia 3, com a caixa de entrada"),
        ]
    }
    fn abas(&self) -> Vec<Aba> {
        vec![Aba::nova("triagem", "Triagem", "T")
            .grupo(Grupo::novo("triagem", "Triagem", &["triagem.descartar"]))
            .grupo(Grupo::novo("lotes", "Lotes", &["lote.enviar"]))]
    }
    fn navegacao(&self) -> Vec<GrupoNavegacao> {
        vec![GrupoNavegacao::novo("Inventário", vec![
            ItemNavegacao::novo("fornecedores", "Fornecedores", "store").contagem(12),
            ItemNavegacao::novo("hoje", "Hoje", "today").indisponivel("fatia 2", "Chega com o livro-caixa"),
        ])]
    }
    fn rota_inicial(&self) -> Rota { Rota::nova("fornecedores") }
    fn trilha(&self, rota: &Rota) -> Vec<Elo> {
        match rota.como_texto() {
            "fornecedor:energia" => vec![Elo::link("Fornecedores", "fornecedores"), Elo::texto("Energia Exemplo")],
            _ => vec![Elo::texto("Fornecedores")],
        }
    }
    fn desenhar_pagina(&mut self, ui: &mut egui::Ui, rota: &Rota, _local: Local) {
        if rota.como_texto() == "fornecedores" {
            link_item(ui, "Energia Exemplo", "fornecedor:energia");
        }
    }
    fn ao_executar(&mut self, _ctx: &egui::Context, id: &str) {
        // grava a decisão etc.
        let _ = id;
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = ConfigCasca::nova(
        "Seu Bolso Feliz",
        include_str!("tokens.json"),
        include_str!("temas.json"),
    );
    config.tema_inicial = "salvia".into();
    Casca::nova(MeuApp, config)?.executar()?;
    Ok(())
}
```

Exemplo completo, com tudo ligado: `cargo run -p bancada-casca --example demonstracao`
(dados fictícios em `tests/comum/app_exemplo.rs`).

## API pública, por parte

| Parte | Tipos e funções | Observações |
|---|---|---|
| App | `AppCasca` | obrigatórios: `acoes`, `abas`, `navegacao`, `rota_inicial`, `trilha`, `desenhar_pagina`, `ao_executar`. O resto tem padrão. |
| Janela | `Casca::nova(app, ConfigCasca)`, `.executar()`, `.desenhar(ui)` | `desenhar` é o quadro inteiro (testes headless usam `Context::run_ui`). `casca.app` e `casca.estado` são públicos. |
| Ações | `Acao` (builder: `icone`, `atalho`, `dica`, `keytip`, `alternancia(marcada)`, `desabilitada(motivo)`, `habilitada_se`, `fora_da_paleta`), `ListaDeAcoes` | lista única: ids únicos e atalho sem conflito (validados a cada quadro; erro vai para a barra de status, sem derrubar a janela). |
| Ação com menu | `Acao::nova("filtro.estado", "Estado").menu(&["filtro.todos", "filtro.abertos"])` | executar a Ação abre o menu com as Ações dadas (da lista única); o app recebe só a escolhida. |
| Ações da casca | `ids::{VOLTAR, AVANCAR, PALETA, MAPA_DE_ATALHOS, RIBBON_RECOLHER, RIBBON_CLASSICO, RIBBON_SIMPLES, DENSIDADE_COMPACTA, TELA_CHEIA, MODO_FOCO}`, `ids::painel("detalhe")` | o app as põe nas abas como qualquer Ação (ex.: as caixas de painel na aba Exibir). Atalhos reservados: Alt+←/→, Ctrl+K, Ctrl+/, Ctrl+F1, F10 (Foco), F11 (tela cheia). Alt+letra abre os KeyTips direto na aba. |
| Atalhos | `Atalho::{tecla, ctrl, alt}`, `.com_shift()`, `Atalho::ler("Ctrl+Shift+K")` | modificadores comparados com exatidão. Com um campo de texto em foco, só disparam os com Ctrl ou Alt. |
| Ribbon | `Aba::nova(id, rótulo, keytip).contextual(visível).grupo(Grupo::novo(id, rótulo, &[ids]))`, `ModoRibbon` | a primeira Ação do grupo é o botão grande no modo clássico. A aba Arquivo (KeyTip `A`) aparece quando `secoes_bastidores()` não é vazio. |
| Bastidores | `SecaoBastidores`, `desenhar_bastidores`, `vista::opcoes_de_aparencia(ui, &textos)` | `opcoes_de_aparencia` desenha Tema (por família), Densidade e Escala. |
| Navegação | `Rota` (opaca), `GrupoNavegacao` (`selo("fatia 1")`), `ItemNavegacao` (`contagem`, `indisponivel(selo, motivo)`, `subitem()`), `Elo`, `Historico`, `Local` | o item ativo acende por igualdade ou prefixo (`fornecedor` acende em `fornecedor:energia`). |
| Link de item | `link_item(ui, rótulo, rota)` | clique abre no mesmo lugar (centro, ou no próprio flutuante); Ctrl+clique ou botão do meio abre num flutuante novo. |
| Página de item | `vista::abas_de_pagina(ui, rota, &[AbaDePagina::nova(id, rótulo).contagem(n).indisponivel(motivo)])` | abas da página (Movimentações, Registros · a conferir, Rastro…); a desabilitada mostra o motivo na dica; a escolha fica guardada por rota. |
| Pedidos | `pedir(ctx, Pedido::…)`, `abrir`, `abrir_flutuante`, `executar`, `pedidos::local_atual(ctx)` | `Pedido::{Abrir, AbrirFlutuante, Voltar, Avancar, Executar, Tema, Densidade, Escala, AbrirPaleta, FecharBastidores, AbrirAba}`. |
| Preferências | `casca.preferencias()`, `casca.aplicar_preferencias(&p)`, `AppCasca::ao_mudar_preferencias` | `Preferencias` (serde/JSON): Tema, Densidade, Escala, modo do Ribbon, painéis visíveis, tela cheia e modo Foco. O app grava (no SBF, pelas preferências do `bancada-armazem`). |
| Paleta | `Destino` (via `AppCasca::destinos`) | busca sem caixa nem acento: prefixo > início de palavra > trecho > subsequência. Itens indisponíveis e Ações desabilitadas aparecem com o motivo e não executam. |
| Dock | `PainelDock::novo(id, título, pilha)` (`icone`, `escondido`), `desenhar_painel` | painéis da mesma `pilha` viram abas; pilhas ficam uma embaixo da outra. O X da aba esconde o painel (volta pela caixa dele, `ids::painel(id)`). |
| Status | `SegmentoStatus::novo(id, texto)` (`icone`, `tom(Tom::Aviso)`, `dica`, `clicavel`, `a_direita`) | clique chega em `ao_clicar_status`. |
| Temas | `ConfigCasca{tokens_json, temas_json, tema_inicial, densidade, escala}`, `TemaAtivo::do_contexto(ctx)`, `Paleta`, `TemaAtivo::cor("hero-bg")` | formato do `tokens.json`/`temas.json` do SBF e do Chess. `Paleta` tem os papéis semânticos em português (`superficie_0..4`, `tinta`, `tinta_suave` = `--text-3`, `sucesso`, `aviso`, `perigo`, `ouro`…). |
| i18n | `Textos::pt_br()`, `.acrescentar(ftl)`, `.t(id)`, `.t_com(id, args)` | textos da casca em `i18n/pt-BR/casca.ftl` (ids `casca-*`). O app acrescenta os dele e passa em `ConfigCasca::textos`. |
| Ícones | `FamiliaDeIcones::com_fonte(bytes).glifos([("delete", "\u{e872}")])` | a casca não embute fonte; sem glifo, o botão mostra só o rótulo. Ícones que a casca usa (com reserva nas fontes do egui se o app não mapear): `voltar`, `avancar`, `paleta`, `atalhos`, `recolher`, `expandir`, `ribbon-classico`, `ribbon-simples`, `densidade`, `fechar`, `tela-cheia`, `foco`. A fonte entra por `add_font` (não apaga as do app); `pronta(ctx)` diz se já vale neste quadro. |
| Fonte da interface | `ConfigCasca::fonte_interface` | bytes da Manrope (SBF), à frente da proporcional padrão. |

## Renderizador e plataformas

- Feature padrão `glow` (OpenGL). `--no-default-features --features wgpu` troca para wgpu.
- Testado no Linux (X11/XWayland), inclusive a janela destacada como janela nativa. No
  Wayland nativo a demonstração não desenhou nenhum quadro dentro do sandbox do agente;
  conferir na sessão do usuário (`WAYLAND_DISPLAY= cargo run …` força o XWayland).

## Decisões

- **Renderizador glow**, não wgpu: compila bem mais rápido e suporta viewports múltiplos.
- **Barra de título**: as decorações são as do sistema; a Barra de Acesso Rápido fica numa faixa
  logo abaixo (barra de título própria, sem decoração, fica para depois).
- **Clique normal num link dentro de um painel flutuante navega o próprio painel** (para
  comparar lado a lado navegando em paralelo); "Abrir no centro" fica no cabeçalho dele.
- **Voltar/Avançar** pelo teclado e pelos botões laterais do mouse valem para o centro; cada
  painel flutuante tem os próprios botões.
- **Conflito de atalho**: o da casca vence; entre duas Ações do app, fica com a primeira.
- **O Ribbon mostra as Ações da área aberta** (ADR-018): o app sabe a rota aberta por
  `ao_abrir_rota` e devolve em `abas()` as abas dela; `Pedido::AbrirAba` troca a aba ativa.

## Testes

`cargo test -p bancada-casca`: modelos puros (Ações, atalhos, Ribbon e modos, KeyTips,
Paleta, histórico, flutuantes, temas) e a janela sem janela (`tests/janela.rs`, com
`egui::Context::run_ui` e entrada sintética), incluindo a carga dos 7 temas a partir do
`temas.json` real do SBF (cópia em `dados-de-exemplo/`).
