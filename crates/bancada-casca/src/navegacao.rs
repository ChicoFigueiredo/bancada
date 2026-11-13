//! Navegação relacional: a `Rota` opaca do app, o histórico Voltar/Avançar, a trilha, os
//! painéis flutuantes e o painel de navegação à esquerda. Tudo puro, testável sem janela.

use std::fmt;

/// Endereço de uma página do app, opaco para a casca (`"fornecedor:energia"`). O app decide o
/// formato; a casca só guarda, compara e devolve.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Rota(String);

impl Rota {
    pub fn nova(texto: impl Into<String>) -> Self {
        Self(texto.into())
    }

    pub fn como_texto(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Rota {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for Rota {
    fn from(texto: &str) -> Self {
        Self::nova(texto)
    }
}

impl From<String> for Rota {
    fn from(texto: String) -> Self {
        Self(texto)
    }
}

/// Histórico de um lugar (o centro ou um painel flutuante): a rota atual, o passado e o futuro.
/// Navegar depois de voltar descarta o futuro, como num navegador.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Historico {
    atual: Rota,
    passado: Vec<Rota>,
    futuro: Vec<Rota>,
}

/// Quantas rotas o passado guarda (as mais antigas saem).
pub const LIMITE_DO_HISTORICO: usize = 200;

impl Historico {
    pub fn novo(inicial: Rota) -> Self {
        Self {
            atual: inicial,
            passado: Vec::new(),
            futuro: Vec::new(),
        }
    }

    pub fn atual(&self) -> &Rota {
        &self.atual
    }

    /// Vai para `rota`. Navegar para a própria rota atual não cria entrada. Devolve se mudou.
    pub fn navegar(&mut self, rota: Rota) -> bool {
        if rota == self.atual {
            return false;
        }
        let anterior = std::mem::replace(&mut self.atual, rota);
        self.passado.push(anterior);
        if self.passado.len() > LIMITE_DO_HISTORICO {
            self.passado.remove(0);
        }
        self.futuro.clear();
        true
    }

    pub fn pode_voltar(&self) -> bool {
        !self.passado.is_empty()
    }

    pub fn pode_avancar(&self) -> bool {
        !self.futuro.is_empty()
    }

    pub fn voltar(&mut self) -> bool {
        let Some(anterior) = self.passado.pop() else {
            return false;
        };
        let atual = std::mem::replace(&mut self.atual, anterior);
        self.futuro.push(atual);
        true
    }

    pub fn avancar(&mut self) -> bool {
        let Some(proxima) = self.futuro.pop() else {
            return false;
        };
        let atual = std::mem::replace(&mut self.atual, proxima);
        self.passado.push(atual);
        true
    }

    /// O passado, do mais antigo ao mais recente (para o menu do botão Voltar).
    pub fn passado(&self) -> &[Rota] {
        &self.passado
    }

    /// O futuro, do mais distante ao mais próximo.
    pub fn futuro(&self) -> &[Rota] {
        &self.futuro
    }
}

/// Um elo da trilha ("Fornecedores › Energia Exemplo"). Sem rota, o elo não é clicável.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Elo {
    pub rotulo: String,
    pub rota: Option<Rota>,
}

impl Elo {
    pub fn link(rotulo: impl Into<String>, rota: impl Into<Rota>) -> Self {
        Self {
            rotulo: rotulo.into(),
            rota: Some(rota.into()),
        }
    }

    pub fn texto(rotulo: impl Into<String>) -> Self {
        Self {
            rotulo: rotulo.into(),
            rota: None,
        }
    }
}

/// Onde uma página é desenhada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Local {
    Centro,
    /// Um painel flutuante, pelo id.
    Flutuante(u64),
}

/// Um painel flutuante: uma página aberta com Ctrl+clique, com o próprio histórico. Pode ser
/// destacado em janela do sistema (viewport do egui).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PainelFlutuante {
    pub id: u64,
    pub historico: Historico,
    pub destacado: bool,
}

/// Os painéis flutuantes abertos, na ordem em que foram abertos.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Flutuantes {
    paineis: Vec<PainelFlutuante>,
    proximo_id: u64,
}

impl Flutuantes {
    /// Abre um painel novo com a rota e devolve o id. Vários podem mostrar a mesma rota (para
    /// comparar duas versões lado a lado o usuário pode querer isso).
    pub fn abrir(&mut self, rota: Rota) -> u64 {
        self.proximo_id += 1;
        let id = self.proximo_id;
        self.paineis.push(PainelFlutuante {
            id,
            historico: Historico::novo(rota),
            destacado: false,
        });
        id
    }

    pub fn fechar(&mut self, id: u64) -> bool {
        let antes = self.paineis.len();
        self.paineis.retain(|p| p.id != id);
        self.paineis.len() != antes
    }

    pub fn get(&self, id: u64) -> Option<&PainelFlutuante> {
        self.paineis.iter().find(|p| p.id == id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut PainelFlutuante> {
        self.paineis.iter_mut().find(|p| p.id == id)
    }

    /// Destaca em janela do sistema, ou devolve para dentro da janela principal.
    pub fn destacar(&mut self, id: u64, destacado: bool) -> bool {
        match self.get_mut(id) {
            Some(p) => {
                p.destacado = destacado;
                true
            }
            None => false,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &PainelFlutuante> {
        self.paineis.iter()
    }

    pub fn ids(&self) -> Vec<u64> {
        self.paineis.iter().map(|p| p.id).collect()
    }

    pub fn len(&self) -> usize {
        self.paineis.len()
    }

    pub fn is_empty(&self) -> bool {
        self.paineis.is_empty()
    }
}

/// Item desabilitado do painel de navegação: selo curto ("fatia 2") e motivo na dica.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Indisponivel {
    pub selo: String,
    pub motivo: String,
}

/// Um item do painel de navegação à esquerda.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemNavegacao {
    pub rota: Rota,
    pub rotulo: String,
    pub icone: String,
    pub contagem: Option<u64>,
    pub indisponivel: Option<Indisponivel>,
    /// Recuo: 0 = item do grupo, 1 = subitem ("Descartados" sob "Documentos").
    pub nivel: u8,
}

impl ItemNavegacao {
    pub fn novo(rota: impl Into<Rota>, rotulo: impl Into<String>, icone: &str) -> Self {
        Self {
            rota: rota.into(),
            rotulo: rotulo.into(),
            icone: icone.to_owned(),
            contagem: None,
            indisponivel: None,
            nivel: 0,
        }
    }

    /// Subitem, recuado sob o item anterior.
    pub fn subitem(mut self) -> Self {
        self.nivel = 1;
        self
    }

    pub fn contagem(mut self, n: u64) -> Self {
        self.contagem = Some(n);
        self
    }

    pub fn indisponivel(mut self, selo: impl Into<String>, motivo: impl Into<String>) -> Self {
        self.indisponivel = Some(Indisponivel {
            selo: selo.into(),
            motivo: motivo.into(),
        });
        self
    }
}

/// Um grupo do painel de navegação ("Dinheiro", "Inventário").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrupoNavegacao {
    pub rotulo: String,
    /// Selo ao lado do nome do grupo ("fatia 1").
    pub selo: Option<String>,
    pub itens: Vec<ItemNavegacao>,
}

impl GrupoNavegacao {
    pub fn novo(rotulo: impl Into<String>, itens: Vec<ItemNavegacao>) -> Self {
        Self {
            rotulo: rotulo.into(),
            selo: None,
            itens,
        }
    }

    pub fn selo(mut self, selo: impl Into<String>) -> Self {
        self.selo = Some(selo.into());
        self
    }
}

/// O item da navegação que corresponde à rota atual: igualdade exata, ou o item cuja rota é o
/// prefixo mais longo seguido de `:` ou `/` (a página `fornecedor:energia` acende
/// "Fornecedores" se o app usar `fornecedor` como rota da área).
pub fn item_ativo<'a>(grupos: &'a [GrupoNavegacao], rota: &Rota) -> Option<&'a ItemNavegacao> {
    let texto = rota.como_texto();
    let mut melhor: Option<&ItemNavegacao> = None;
    for item in grupos.iter().flat_map(|g| &g.itens) {
        let r = item.rota.como_texto();
        if r == texto {
            return Some(item);
        }
        let prefixo = texto.len() > r.len()
            && texto.starts_with(r)
            && matches!(texto.as_bytes()[r.len()], b':' | b'/');
        if prefixo && melhor.is_none_or(|m| m.rota.como_texto().len() < r.len()) {
            melhor = Some(item);
        }
    }
    melhor
}

#[cfg(test)]
mod testes {
    use super::*;

    fn r(t: &str) -> Rota {
        Rota::nova(t)
    }

    #[test]
    fn voltar_e_avancar_refazem_o_caminho() {
        let mut h = Historico::novo(r("inicio"));
        assert!(!h.pode_voltar());
        h.navegar(r("fornecedores"));
        h.navegar(r("fornecedor:energia"));
        assert!(h.voltar());
        assert_eq!(h.atual(), &r("fornecedores"));
        assert!(h.voltar());
        assert_eq!(h.atual(), &r("inicio"));
        assert!(!h.voltar());
        assert!(h.avancar());
        assert!(h.avancar());
        assert_eq!(h.atual(), &r("fornecedor:energia"));
        assert!(!h.avancar());
    }

    #[test]
    fn navegar_depois_de_voltar_descarta_o_futuro() {
        let mut h = Historico::novo(r("a"));
        h.navegar(r("b"));
        h.navegar(r("c"));
        h.voltar();
        assert!(h.pode_avancar());
        h.navegar(r("d"));
        assert!(!h.pode_avancar());
        assert_eq!(h.passado(), &[r("a"), r("b")]);
        h.voltar();
        assert_eq!(h.atual(), &r("b"));
    }

    #[test]
    fn navegar_para_a_mesma_rota_nao_empilha() {
        let mut h = Historico::novo(r("a"));
        assert!(!h.navegar(r("a")));
        assert!(!h.pode_voltar());
    }

    #[test]
    fn historico_tem_limite() {
        let mut h = Historico::novo(r("0"));
        for i in 1..=LIMITE_DO_HISTORICO + 10 {
            h.navegar(r(&i.to_string()));
        }
        assert_eq!(h.passado().len(), LIMITE_DO_HISTORICO);
    }

    #[test]
    fn varios_flutuantes_cada_um_com_a_propria_rota() {
        let mut f = Flutuantes::default();
        let a = f.abrir(r("fornecedor:energia"));
        let b = f.abrir(r("fornecedor:agua"));
        assert_ne!(a, b);
        assert_eq!(f.len(), 2);
        f.get_mut(a).unwrap().historico.navegar(r("registro:7"));
        assert_eq!(f.get(a).unwrap().historico.atual(), &r("registro:7"));
        assert_eq!(f.get(b).unwrap().historico.atual(), &r("fornecedor:agua"));
        assert!(f.destacar(b, true));
        assert!(f.get(b).unwrap().destacado);
        assert!(f.fechar(a));
        assert!(!f.fechar(a));
        assert_eq!(f.ids(), vec![b]);
        let c = f.abrir(r("x"));
        assert!(c > b, "ids não são reaproveitados");
    }

    #[test]
    fn item_ativo_por_igualdade_ou_prefixo() {
        let grupos = vec![GrupoNavegacao::novo(
            "Dinheiro",
            vec![
                ItemNavegacao::novo("fornecedor", "Fornecedores", ""),
                ItemNavegacao::novo("fornecedor:energia:x", "Mais longo", ""),
                ItemNavegacao::novo("ciclo", "Ciclos", ""),
            ],
        )];
        assert_eq!(
            item_ativo(&grupos, &r("ciclo")).map(|i| i.rotulo.as_str()),
            Some("Ciclos")
        );
        assert_eq!(
            item_ativo(&grupos, &r("fornecedor:energia")).map(|i| i.rotulo.as_str()),
            Some("Fornecedores")
        );
        assert!(item_ativo(&grupos, &r("fornecedores")).is_none());
    }
}
