//! O contrato entre a casca e o app: o trait [`AppCasca`] e os tipos que ele devolve.

use crate::acoes::{Acao, IdAcao};
use crate::navegacao::{Elo, GrupoNavegacao, Local, Rota};
use crate::paleta::Destino;
use crate::ribbon::Aba;

/// Tom de um segmento da barra de status. Cor nunca é o único sinal: um segmento de aviso
/// leva ícone ou palavra.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tom {
    #[default]
    Neutro,
    Info,
    Sucesso,
    Aviso,
    Perigo,
}

/// Um segmento da barra de status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentoStatus {
    pub id: String,
    pub texto: String,
    pub icone: String,
    pub tom: Tom,
    pub dica: String,
    /// Clicável: o clique chega em [`AppCasca::ao_clicar_status`].
    pub clicavel: bool,
    /// Fica no lado direito da barra.
    pub a_direita: bool,
}

impl SegmentoStatus {
    pub fn novo(id: impl Into<String>, texto: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            texto: texto.into(),
            icone: String::new(),
            tom: Tom::Neutro,
            dica: String::new(),
            clicavel: false,
            a_direita: false,
        }
    }

    pub fn icone(mut self, nome: impl Into<String>) -> Self {
        self.icone = nome.into();
        self
    }

    pub fn tom(mut self, tom: Tom) -> Self {
        self.tom = tom;
        self
    }

    pub fn dica(mut self, dica: impl Into<String>) -> Self {
        self.dica = dica.into();
        self
    }

    pub fn clicavel(mut self) -> Self {
        self.clicavel = true;
        self
    }

    pub fn a_direita(mut self) -> Self {
        self.a_direita = true;
        self
    }
}

/// Um painel do dock à direita (no SBF: Detalhe, Origens, Texto, Histórico de decisões).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PainelDock {
    pub id: String,
    pub titulo: String,
    pub icone: String,
    /// Visível na primeira abertura.
    pub visivel_por_padrao: bool,
    /// Painéis com a mesma pilha ficam como abas do mesmo grupo; pilhas diferentes ficam
    /// empilhadas de cima para baixo, na ordem do número.
    pub pilha: u8,
}

impl PainelDock {
    pub fn novo(id: impl Into<String>, titulo: impl Into<String>, pilha: u8) -> Self {
        Self {
            id: id.into(),
            titulo: titulo.into(),
            icone: String::new(),
            visivel_por_padrao: true,
            pilha,
        }
    }

    pub fn icone(mut self, nome: impl Into<String>) -> Self {
        self.icone = nome.into();
        self
    }

    pub fn escondido(mut self) -> Self {
        self.visivel_por_padrao = false;
        self
    }
}

/// Uma seção dos bastidores (aba Arquivo), que o app desenha.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecaoBastidores {
    pub id: String,
    pub rotulo: String,
    pub icone: String,
}

impl SecaoBastidores {
    pub fn nova(id: impl Into<String>, rotulo: impl Into<String>, icone: &str) -> Self {
        Self {
            id: id.into(),
            rotulo: rotulo.into(),
            icone: icone.to_owned(),
        }
    }
}

/// O app que a casca hospeda. A casca pergunta (Ações, abas, navegação, painéis, status) a cada
/// quadro e avisa (Ação executada, rota aberta). Para pedir algo à casca de dentro do desenho
/// (abrir uma rota, executar uma Ação, trocar o Tema), use as funções de [`crate::pedidos`] e o
/// widget [`crate::link_item`].
pub trait AppCasca {
    /// A lista única de Ações do app, com o estado deste quadro. A casca acrescenta as dela
    /// (ids `casca.*`).
    fn acoes(&self) -> Vec<Acao>;

    /// As abas do Ribbon (normais e contextuais), com os grupos referenciando ids de Ações.
    fn abas(&self) -> Vec<Aba>;

    /// Ações da Barra de Acesso Rápido, na barra de título.
    fn acesso_rapido(&self) -> Vec<IdAcao> {
        Vec::new()
    }

    /// Os grupos do painel de navegação à esquerda.
    fn navegacao(&self) -> Vec<GrupoNavegacao>;

    /// Destinos de navegação a mais para a Paleta (além dos itens da navegação).
    fn destinos(&self) -> Vec<Destino> {
        Vec::new()
    }

    /// A rota aberta no centro quando a janela abre.
    fn rota_inicial(&self) -> Rota;

    /// A trilha da rota ("Fornecedores › Energia Exemplo"). O último elo é a página atual.
    fn trilha(&self, rota: &Rota) -> Vec<Elo>;

    /// Título curto da rota (janela destacada, cabeçalho do painel flutuante, menu Voltar).
    fn titulo_da_rota(&self, rota: &Rota) -> String {
        self.trilha(rota)
            .last()
            .map(|e| e.rotulo.clone())
            .unwrap_or_else(|| rota.to_string())
    }

    /// Desenha a página da rota no centro ou num painel flutuante.
    fn desenhar_pagina(&mut self, ui: &mut egui::Ui, rota: &Rota, local: Local);

    /// Os painéis do dock.
    fn paineis(&self) -> Vec<PainelDock> {
        Vec::new()
    }

    /// Desenha um painel do dock.
    fn desenhar_painel(&mut self, ui: &mut egui::Ui, id: &str) {
        let _ = (ui, id);
    }

    /// Os segmentos da barra de status.
    fn status(&self) -> Vec<SegmentoStatus> {
        Vec::new()
    }

    /// As seções dos bastidores. Vazio = sem aba Arquivo.
    fn secoes_bastidores(&self) -> Vec<SecaoBastidores> {
        Vec::new()
    }

    /// Desenha uma seção dos bastidores. Para a seção de Opções, o app pode chamar
    /// [`crate::vista::opcoes_de_aparencia`].
    fn desenhar_bastidores(&mut self, ui: &mut egui::Ui, id: &str) {
        let _ = (ui, id);
    }

    /// Uma Ação do app foi executada (Ribbon, atalho, KeyTip, Paleta ou Barra de Acesso Rápido).
    /// As Ações `casca.*` a casca trata sozinha e não chegam aqui.
    fn ao_executar(&mut self, ctx: &egui::Context, id: &str);

    /// Uma rota foi aberta (no centro ou num painel flutuante), por qualquer caminho.
    fn ao_abrir_rota(&mut self, ctx: &egui::Context, rota: &Rota, local: Local) {
        let _ = (ctx, rota, local);
    }

    /// Um segmento clicável da barra de status foi clicado.
    fn ao_clicar_status(&mut self, ctx: &egui::Context, id: &str) {
        let _ = (ctx, id);
    }
}
