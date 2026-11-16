//! # bancada-casca
//!
//! A shell desktop compartilhada pelo Seu Bolso Feliz e pelo Chess Studio (ADR-018 do SBF),
//! em egui/eframe puro, sem Bevy e sem domínio. O app é um `impl` de [`AppCasca`]: fornece a
//! lista única de Ações, as abas do Ribbon, a navegação, os painéis do dock, a barra de status
//! e o desenho das páginas; a casca devolve os eventos (Ação executada, rota aberta).
//!
//! De cima para baixo: barra de título com a Barra de Acesso Rápido; Ribbon (clássico, simples
//! e recolhido, KeyTips com Alt, abas contextuais, bastidores na aba Arquivo); painel de
//! navegação à esquerda; trilha com Voltar/Avançar e o centro; dock com egui_tiles à direita;
//! barra de status. Por cima: Paleta (Ctrl+K), Mapa de atalhos (Ctrl+/) e painéis flutuantes
//! (Ctrl+clique num [`link_item`]), que podem ser destacados em janela do sistema.
//!
//! Veja o `README.md` do crate para o exemplo mínimo e `examples/demonstracao.rs`.

pub mod acoes;
pub mod app;
pub mod atalhos;
pub mod casca;
pub mod i18n;
pub mod icones;
pub mod navegacao;
pub mod paleta;
pub mod pedidos;
pub mod ribbon;
pub mod tema;
pub mod vista;

pub use acoes::{Acao, Disponibilidade, IdAcao, ListaDeAcoes, TipoAcao};
pub use app::{AppCasca, PainelDock, SecaoBastidores, SegmentoStatus, Tom};
pub use atalhos::Atalho;
pub use casca::{Casca, ConfigCasca, Estado, Preferencias, ids};
pub use i18n::Textos;
pub use icones::FamiliaDeIcones;
pub use navegacao::{Elo, GrupoNavegacao, Historico, ItemNavegacao, Local, Rota};
pub use paleta::Destino;
pub use pedidos::{Pedido, abrir, abrir_flutuante, executar, link_item, mostrar_painel, pedir};
pub use ribbon::{Aba, Grupo, ModoRibbon};
pub use tema::{Densidade, Paleta, TemaAtivo};

// Reexportados para o app usar as mesmas versões.
pub use eframe;
pub use egui;
