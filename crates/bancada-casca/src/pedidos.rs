//! Pedidos do app à casca, feitos de dentro do desenho só com o `egui::Context`: abrir uma rota,
//! abrir num painel flutuante, executar uma Ação, trocar o Tema. Ficam numa fila no `Context` e
//! a casca os atende no fim do quadro. E o widget de **link de item**, que todo nome usa.

use crate::navegacao::{Local, Rota};
use crate::tema::Densidade;
use egui::{Response, Ui};

/// Um pedido à casca.
#[derive(Debug, Clone, PartialEq)]
pub enum Pedido {
    /// Abre a rota num lugar (o centro ou um painel flutuante já aberto).
    Abrir {
        rota: Rota,
        local: Local,
    },
    /// Abre a rota num painel flutuante novo.
    AbrirFlutuante(Rota),
    Voltar(Local),
    Avancar(Local),
    /// Executa uma Ação da lista (do app ou `casca.*`).
    Executar(String),
    Tema(String),
    Densidade(Densidade),
    Escala(f32),
    AbrirPaleta,
    FecharBastidores,
    /// Torna ativa uma aba do Ribbon (por exemplo a contextual que acabou de aparecer).
    AbrirAba(String),
    /// Mostra um painel do dock e o torna a aba ativa da pilha dele.
    MostrarPainel(String),
}

fn id_fila() -> egui::Id {
    egui::Id::new("bancada-casca.pedidos")
}

fn id_local() -> egui::Id {
    egui::Id::new("bancada-casca.local-atual")
}

/// Põe um pedido na fila.
pub fn pedir(ctx: &egui::Context, pedido: Pedido) {
    ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Vec<Pedido>>(id_fila())
            .push(pedido);
    });
    ctx.request_repaint();
}

/// Abre a rota no centro.
pub fn abrir(ctx: &egui::Context, rota: impl Into<Rota>) {
    pedir(
        ctx,
        Pedido::Abrir {
            rota: rota.into(),
            local: Local::Centro,
        },
    );
}

/// Abre a rota num painel flutuante novo.
pub fn abrir_flutuante(ctx: &egui::Context, rota: impl Into<Rota>) {
    pedir(ctx, Pedido::AbrirFlutuante(rota.into()));
}

/// Mostra o painel do dock (por id) e o torna a aba ativa da pilha dele.
pub fn mostrar_painel(ctx: &egui::Context, id: impl Into<String>) {
    pedir(ctx, Pedido::MostrarPainel(id.into()));
}

/// Executa uma Ação da lista única.
pub fn executar(ctx: &egui::Context, id: impl Into<String>) {
    pedir(ctx, Pedido::Executar(id.into()));
}

/// Tira todos os pedidos da fila (a casca chama no fim do quadro).
pub fn drenar(ctx: &egui::Context) -> Vec<Pedido> {
    ctx.data_mut(|d| std::mem::take(d.get_temp_mut_or_default::<Vec<Pedido>>(id_fila())))
}

/// Em que lugar a página sendo desenhada está (a casca define antes de chamar o app).
pub fn local_atual(ctx: &egui::Context) -> Local {
    ctx.data(|d| d.get_temp::<Local>(id_local()))
        .unwrap_or(Local::Centro)
}

pub(crate) fn definir_local(ctx: &egui::Context, local: Local) {
    ctx.data_mut(|d| d.insert_temp(id_local(), local));
}

/// O que um clique num link pede, dado o lugar onde o link está e se o Ctrl estava apertado.
/// Clique normal abre no mesmo lugar (no centro, ou no próprio painel flutuante, para navegar
/// nele em paralelo); Ctrl+clique (ou clique do meio) abre num painel flutuante novo.
pub fn pedido_do_clique(rota: Rota, local: Local, ctrl: bool) -> Pedido {
    if ctrl {
        Pedido::AbrirFlutuante(rota)
    } else {
        Pedido::Abrir { rota, local }
    }
}

/// Link de item: o nome de uma entidade, clicável. Clique abre a página; Ctrl+clique (ou o
/// botão do meio) abre num painel flutuante. Use em qualquer lugar: tabela, cabeçalho, dock.
pub fn link_item(
    ui: &mut Ui,
    rotulo: impl Into<egui::WidgetText>,
    rota: impl Into<Rota>,
) -> Response {
    let rota = rota.into();
    let resposta = ui
        .add(egui::Link::new(rotulo))
        .on_hover_text(rota.to_string());
    let ctrl = ui.input(|i| i.modifiers.command);
    if resposta.clicked() {
        let local = local_atual(ui.ctx());
        pedir(ui.ctx(), pedido_do_clique(rota, local, ctrl));
    } else if resposta.middle_clicked() {
        pedir(ui.ctx(), Pedido::AbrirFlutuante(rota));
    }
    resposta
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn clique_abre_no_lugar_e_ctrl_clique_abre_flutuante() {
        let r = Rota::nova("fornecedor:energia");
        assert_eq!(
            pedido_do_clique(r.clone(), Local::Centro, false),
            Pedido::Abrir {
                rota: r.clone(),
                local: Local::Centro
            }
        );
        assert_eq!(
            pedido_do_clique(r.clone(), Local::Flutuante(3), false),
            Pedido::Abrir {
                rota: r.clone(),
                local: Local::Flutuante(3)
            }
        );
        assert_eq!(
            pedido_do_clique(r.clone(), Local::Centro, true),
            Pedido::AbrirFlutuante(r)
        );
    }

    #[test]
    fn fila_de_pedidos_no_contexto() {
        let ctx = egui::Context::default();
        abrir(&ctx, "a");
        executar(&ctx, "x");
        let p = drenar(&ctx);
        assert_eq!(p.len(), 2);
        assert!(drenar(&ctx).is_empty());
    }
}
