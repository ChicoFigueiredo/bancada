//! Modelo puro do Ribbon (sem desenho): abas do app (normais e contextuais), grupos que
//! referenciam a lista única de Ações, os três modos (clássico, simples e recolhido), o corte
//! do modo simples e a máquina de estados dos KeyTips. Porte do `ribbon/model.rs` do Chess
//! Studio, agora parametrizado pelo app.

use crate::acoes::{IdAcao, ListaDeAcoes};
use std::collections::HashSet;
use std::fmt;

/// Id da aba Arquivo, que a casca cria quando o app tem seções de bastidores.
pub const ABA_ARQUIVO: &str = "casca.arquivo";
/// KeyTip reservado para a aba Arquivo.
pub const KEYTIP_ARQUIVO: &str = "A";

/// Um grupo do corpo de uma aba: rótulo e as Ações, na ordem. A primeira Ação é o botão
/// grande do modo clássico.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grupo {
    pub id: String,
    pub rotulo: String,
    pub acoes: Vec<IdAcao>,
}

impl Grupo {
    pub fn novo(id: impl Into<String>, rotulo: impl Into<String>, acoes: &[&str]) -> Self {
        Self {
            id: id.into(),
            rotulo: rotulo.into(),
            acoes: acoes.iter().map(|a| (*a).to_owned()).collect(),
        }
    }
}

/// Uma aba do Ribbon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aba {
    pub id: String,
    pub rotulo: String,
    /// Letra(s) do KeyTip, únicas entre as abas visíveis.
    pub keytip: String,
    /// Aba contextual: só aparece quando `visivel` (por exemplo "Achado", com um selecionado).
    pub contextual: bool,
    pub visivel: bool,
    pub grupos: Vec<Grupo>,
}

impl Aba {
    pub fn nova(id: impl Into<String>, rotulo: impl Into<String>, keytip: &str) -> Self {
        Self {
            id: id.into(),
            rotulo: rotulo.into(),
            keytip: keytip.to_ascii_uppercase(),
            contextual: false,
            visivel: true,
            grupos: Vec::new(),
        }
    }

    /// Aba contextual, visível só quando `visivel`.
    pub fn contextual(mut self, visivel: bool) -> Self {
        self.contextual = true;
        self.visivel = visivel;
        self
    }

    pub fn grupo(mut self, grupo: Grupo) -> Self {
        self.grupos.push(grupo);
        self
    }

    pub fn acoes(&self) -> impl Iterator<Item = &str> {
        self.grupos
            .iter()
            .flat_map(|g| g.acoes.iter().map(String::as_str))
    }
}

/// Por que o conjunto de abas não é válido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroAbas {
    IdRepetido(String),
    KeyTipRepetido(String),
    /// Aba que referencia uma Ação que não está na lista única.
    AcaoInexistente {
        aba: String,
        acao: String,
    },
    /// Duas Ações da mesma aba com o mesmo KeyTip, ou um KeyTip prefixo de outro.
    KeyTipDeAcaoAmbiguo {
        aba: String,
        keytip: String,
    },
}

impl fmt::Display for ErroAbas {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErroAbas::IdRepetido(id) => write!(f, "aba repetida: {id}"),
            ErroAbas::KeyTipRepetido(k) => write!(f, "KeyTip de aba repetido: {k}"),
            ErroAbas::AcaoInexistente { aba, acao } => {
                write!(f, "a aba {aba} usa a Ação {acao}, que não está na lista")
            }
            ErroAbas::KeyTipDeAcaoAmbiguo { aba, keytip } => {
                write!(f, "KeyTip ambíguo na aba {aba}: {keytip}")
            }
        }
    }
}

impl std::error::Error for ErroAbas {}

/// Confere as abas contra a lista de Ações: ids e KeyTips únicos, toda Ação referenciada existe
/// e, dentro de cada aba, nenhum KeyTip repete nem é prefixo de outro (senão o de duas letras
/// nunca seria alcançado).
pub fn validar_abas(abas: &[Aba], acoes: &ListaDeAcoes) -> Result<(), ErroAbas> {
    let mut ids = HashSet::new();
    let mut keytips = HashSet::from([KEYTIP_ARQUIVO.to_owned()]);
    for aba in abas {
        if !ids.insert(aba.id.as_str()) {
            return Err(ErroAbas::IdRepetido(aba.id.clone()));
        }
        if !aba.keytip.is_empty() && !keytips.insert(aba.keytip.clone()) {
            return Err(ErroAbas::KeyTipRepetido(aba.keytip.clone()));
        }
        let mut letras: Vec<&str> = Vec::new();
        let mut vistas = HashSet::new();
        for id in aba.acoes() {
            // A mesma Ação em dois grupos da aba usa o mesmo KeyTip: não é ambiguidade.
            if !vistas.insert(id) {
                continue;
            }
            let Some(acao) = acoes.get(id) else {
                return Err(ErroAbas::AcaoInexistente {
                    aba: aba.id.clone(),
                    acao: id.to_owned(),
                });
            };
            if acao.keytip.is_empty() {
                continue;
            }
            if letras
                .iter()
                .any(|l| l.starts_with(&acao.keytip) || acao.keytip.starts_with(l))
            {
                return Err(ErroAbas::KeyTipDeAcaoAmbiguo {
                    aba: aba.id.clone(),
                    keytip: acao.keytip.clone(),
                });
            }
            letras.push(&acao.keytip);
        }
    }
    Ok(())
}

/// O que uma aba mostra na faixa: a aba Arquivo (bastidores) ou uma aba do app.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Guia {
    Arquivo,
    Aba(String),
}

/// As guias visíveis, na ordem: Arquivo (se houver bastidores), as normais e as contextuais
/// visíveis (as contextuais sempre depois das normais, como no Office).
pub fn guias_visiveis(abas: &[Aba], com_arquivo: bool) -> Vec<Guia> {
    let mut guias = Vec::new();
    if com_arquivo {
        guias.push(Guia::Arquivo);
    }
    guias.extend(
        abas.iter()
            .filter(|a| !a.contextual && a.visivel)
            .map(|a| Guia::Aba(a.id.clone())),
    );
    guias.extend(
        abas.iter()
            .filter(|a| a.contextual && a.visivel)
            .map(|a| Guia::Aba(a.id.clone())),
    );
    guias
}

/// Modo do Ribbon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModoRibbon {
    /// Grupos com botão grande e rótulos de grupo.
    #[default]
    Classico,
    /// Uma linha só; o que não cabe vai para o menu "⋯".
    Simples,
    /// Só as abas; o corpo abre por cima ao clicar numa aba e fecha ao executar.
    Recolhido,
}

/// Estado do Ribbon que persiste entre quadros.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EstadoRibbon {
    pub modo: ModoRibbon,
    /// Modo a que o Ctrl+F1 volta ao expandir.
    pub modo_expandido: ModoRibbon,
    /// Aba ativa (id do app); `None` até a primeira escolha.
    pub aba_ativa: Option<String>,
    /// No modo recolhido: o corpo está aberto por cima.
    pub corpo_aberto: bool,
    pub bastidores_abertos: bool,
}

impl EstadoRibbon {
    /// Ctrl+F1: recolhe ou volta ao último modo expandido.
    pub fn alternar_recolhido(&mut self) {
        if self.modo == ModoRibbon::Recolhido {
            self.modo = self.modo_expandido_ou_padrao();
        } else {
            self.modo_expandido = self.modo;
            self.modo = ModoRibbon::Recolhido;
        }
        self.corpo_aberto = false;
    }

    fn modo_expandido_ou_padrao(&self) -> ModoRibbon {
        match self.modo_expandido {
            ModoRibbon::Recolhido => ModoRibbon::Classico,
            outro => outro,
        }
    }

    pub fn definir_modo(&mut self, modo: ModoRibbon) {
        if modo != ModoRibbon::Recolhido {
            self.modo_expandido = modo;
        }
        self.modo = modo;
        self.corpo_aberto = false;
    }

    /// Clique numa guia. Arquivo abre os bastidores. Duplo clique numa aba alterna o recolhido
    /// (Office). No modo recolhido, o clique abre ou fecha o corpo por cima.
    pub fn clicar_guia(&mut self, guia: &Guia, duplo: bool) {
        match guia {
            Guia::Arquivo => {
                self.bastidores_abertos = true;
                self.corpo_aberto = false;
            }
            Guia::Aba(id) => {
                self.bastidores_abertos = false;
                if duplo {
                    self.aba_ativa = Some(id.clone());
                    self.alternar_recolhido();
                    return;
                }
                if self.modo == ModoRibbon::Recolhido {
                    let mesma = self.aba_ativa.as_deref() == Some(id.as_str());
                    self.corpo_aberto = !(mesma && self.corpo_aberto);
                }
                self.aba_ativa = Some(id.clone());
            }
        }
    }

    /// Depois de executar uma Ação: no modo recolhido o corpo fecha.
    pub fn apos_executar(&mut self) {
        if self.modo == ModoRibbon::Recolhido {
            self.corpo_aberto = false;
        }
    }

    /// Mostra o corpo da aba ativa neste quadro?
    pub fn mostra_corpo(&self) -> bool {
        !self.bastidores_abertos && (self.modo != ModoRibbon::Recolhido || self.corpo_aberto)
    }

    /// A aba ativa entre as visíveis; se a escolhida sumiu (contextual que fechou), a primeira.
    pub fn aba_efetiva<'a>(&self, guias: &'a [Guia]) -> Option<&'a str> {
        let abas = guias.iter().filter_map(|g| match g {
            Guia::Aba(id) => Some(id.as_str()),
            Guia::Arquivo => None,
        });
        let mut primeira = None;
        for id in abas {
            if Some(id) == self.aba_ativa.as_deref() {
                return Some(id);
            }
            primeira.get_or_insert(id);
        }
        primeira
    }
}

// --- Modo simples: o que cabe na linha ---

/// Resultado do corte da linha única do modo simples.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Corte {
    /// Quantos grupos (do começo) ficam visíveis na linha.
    pub visiveis: usize,
    /// Índices dos grupos que vão para o menu "⋯", na ordem original.
    pub excedentes: Vec<usize>,
}

/// Decide quais grupos cabem em `disponivel` pontos. `larguras` são as dos grupos, `folga` o
/// espaço entre eles e `largura_mais` a do botão "⋯". Se tudo cabe, não há "⋯"; senão os grupos
/// saem da direita para a esquerda até sobrar espaço para o "⋯".
pub fn cortar(larguras: &[f32], folga: f32, disponivel: f32, largura_mais: f32) -> Corte {
    let total = |n: usize| -> f32 {
        larguras[..n].iter().sum::<f32>() + folga * n.saturating_sub(1) as f32
    };
    let n = larguras.len();
    if total(n) <= disponivel {
        return Corte {
            visiveis: n,
            excedentes: Vec::new(),
        };
    }
    let orcamento = disponivel - largura_mais - folga;
    let visiveis = (0..=n).rev().find(|&c| total(c) <= orcamento).unwrap_or(0);
    Corte {
        visiveis,
        excedentes: (visiveis..n).collect(),
    }
}

// --- KeyTips ---

/// Em que nível dos KeyTips o usuário está.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum NivelKeyTip {
    #[default]
    Desligado,
    /// Letras sobre as guias (e sobre a Barra de Acesso Rápido, com números).
    Guias,
    /// Letras sobre as Ações da aba.
    Acoes(String),
}

/// Entrada da máquina dos KeyTips.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntradaKeyTip {
    /// Alt apertado e solto sozinho.
    ToqueAlt,
    Esc,
    Letra(char),
}

/// O que a interface faz em resposta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultadoKeyTip {
    Nada,
    /// Torna a aba a ativa (e abre o corpo, se recolhido).
    AbrirAba(String),
    AbrirBastidores,
    Executar(IdAcao),
}

/// Máquina dos KeyTips (Office): Alt → letra da guia → letras da Ação; Esc sobe um nível. As
/// letras são as da Ação, nunca geradas pela posição. Na Barra de Acesso Rápido valem os
/// números 1 a 9, no nível das guias.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyTips {
    nivel: NivelKeyTip,
    digitado: String,
}

impl KeyTips {
    pub fn nivel(&self) -> &NivelKeyTip {
        &self.nivel
    }

    pub fn ativo(&self) -> bool {
        self.nivel != NivelKeyTip::Desligado
    }

    pub fn digitado(&self) -> &str {
        &self.digitado
    }

    pub fn fechar(&mut self) {
        self.nivel = NivelKeyTip::Desligado;
        self.digitado.clear();
    }

    /// Processa uma entrada. `abas` dá as abas do app (para as letras e as Ações), `guias` as
    /// visíveis, `acesso_rapido` as Ações da barra e `acoes` a lista (uma Ação desabilitada não
    /// dispara e o que foi digitado é descartado).
    pub fn tratar(
        &mut self,
        entrada: EntradaKeyTip,
        abas: &[Aba],
        guias: &[Guia],
        acesso_rapido: &[IdAcao],
        acoes: &ListaDeAcoes,
    ) -> ResultadoKeyTip {
        match (entrada, self.nivel.clone()) {
            (EntradaKeyTip::ToqueAlt, NivelKeyTip::Desligado) => {
                self.nivel = NivelKeyTip::Guias;
                self.digitado.clear();
                ResultadoKeyTip::Nada
            }
            (EntradaKeyTip::ToqueAlt, _) => {
                self.fechar();
                ResultadoKeyTip::Nada
            }
            (EntradaKeyTip::Esc, NivelKeyTip::Desligado) => ResultadoKeyTip::Nada,
            (EntradaKeyTip::Esc, NivelKeyTip::Guias) => {
                self.fechar();
                ResultadoKeyTip::Nada
            }
            (EntradaKeyTip::Esc, NivelKeyTip::Acoes(_)) => {
                if self.digitado.is_empty() {
                    self.nivel = NivelKeyTip::Guias;
                }
                self.digitado.clear();
                ResultadoKeyTip::Nada
            }
            (EntradaKeyTip::Letra(_), NivelKeyTip::Desligado) => ResultadoKeyTip::Nada,
            (EntradaKeyTip::Letra(c), NivelKeyTip::Guias) => {
                if let Some(d) = c.to_digit(10)
                    && d >= 1
                    && let Some(id) = acesso_rapido.get(d as usize - 1)
                {
                    self.fechar();
                    return if acoes.esta_habilitada(id) {
                        ResultadoKeyTip::Executar(id.clone())
                    } else {
                        ResultadoKeyTip::Nada
                    };
                }
                self.digitado.push(c.to_ascii_uppercase());
                let letra_da = |g: &Guia| -> String {
                    match g {
                        Guia::Arquivo => KEYTIP_ARQUIVO.to_owned(),
                        Guia::Aba(id) => abas
                            .iter()
                            .find(|a| &a.id == id)
                            .map(|a| a.keytip.clone())
                            .unwrap_or_default(),
                    }
                };
                if let Some(guia) = guias
                    .iter()
                    .find(|g| !letra_da(g).is_empty() && letra_da(g) == self.digitado)
                {
                    self.digitado.clear();
                    return match guia {
                        Guia::Arquivo => {
                            self.fechar();
                            ResultadoKeyTip::AbrirBastidores
                        }
                        Guia::Aba(id) => {
                            self.nivel = NivelKeyTip::Acoes(id.clone());
                            ResultadoKeyTip::AbrirAba(id.clone())
                        }
                    };
                }
                if !guias
                    .iter()
                    .any(|g| letra_da(g).starts_with(&self.digitado))
                {
                    self.digitado.clear();
                }
                ResultadoKeyTip::Nada
            }
            (EntradaKeyTip::Letra(c), NivelKeyTip::Acoes(aba_id)) => {
                self.digitado.push(c.to_ascii_uppercase());
                let Some(aba) = abas.iter().find(|a| a.id == aba_id) else {
                    self.fechar();
                    return ResultadoKeyTip::Nada;
                };
                let letras = |id: &str| acoes.get(id).map(|a| a.keytip.clone()).unwrap_or_default();
                if let Some(id) = aba
                    .acoes()
                    .find(|id| !letras(id).is_empty() && letras(id) == self.digitado)
                {
                    self.digitado.clear();
                    if acoes.esta_habilitada(id) {
                        self.fechar();
                        return ResultadoKeyTip::Executar(id.to_owned());
                    }
                    return ResultadoKeyTip::Nada;
                }
                if !aba
                    .acoes()
                    .any(|id| !letras(id).is_empty() && letras(id).starts_with(&self.digitado))
                {
                    self.digitado.clear();
                }
                ResultadoKeyTip::Nada
            }
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::acoes::Acao;

    fn acoes() -> ListaDeAcoes {
        ListaDeAcoes::nova(vec![
            Acao::nova("doc", "Documento").keytip("D"),
            Acao::nova("descartar", "Descartar").keytip("X"),
            Acao::nova("duvida", "Em dúvida").keytip("UD"),
            Acao::nova("enviar", "Enviar")
                .keytip("E")
                .desabilitada("fatia 3"),
            Acao::nova("abrir", "Abrir original").keytip("O"),
        ])
        .unwrap()
    }

    fn abas() -> Vec<Aba> {
        vec![
            Aba::nova("inicio", "Início", "I"),
            Aba::nova("triagem", "Triagem", "T")
                .grupo(Grupo::novo("t", "Triagem", &["doc", "descartar", "duvida"]))
                .grupo(Grupo::novo("l", "Lotes", &["enviar"])),
            Aba::nova("achado", "Achado", "H")
                .contextual(false)
                .grupo(Grupo::novo("a", "Achado", &["abrir"])),
        ]
    }

    #[test]
    fn abas_contextuais_aparecem_so_quando_o_app_diz_e_depois_das_normais() {
        let mut abas = abas();
        let guias = guias_visiveis(&abas, true);
        assert_eq!(
            guias,
            vec![
                Guia::Arquivo,
                Guia::Aba("inicio".into()),
                Guia::Aba("triagem".into())
            ]
        );
        abas[2].visivel = true;
        abas.swap(0, 2);
        let guias = guias_visiveis(&abas, false);
        assert_eq!(guias.last(), Some(&Guia::Aba("achado".into())));
    }

    #[test]
    fn validacao_pega_acao_inexistente_e_keytip_ambiguo() {
        let lista = acoes();
        assert_eq!(validar_abas(&abas(), &lista), Ok(()));
        let ruim = vec![Aba::nova("x", "X", "X").grupo(Grupo::novo("g", "G", &["nao-existe"]))];
        assert!(matches!(
            validar_abas(&ruim, &lista),
            Err(ErroAbas::AcaoInexistente { .. })
        ));
        let lista2 = ListaDeAcoes::nova(vec![
            Acao::nova("a", "A").keytip("D"),
            Acao::nova("b", "B").keytip("DS"),
        ])
        .unwrap();
        let ambigua = vec![Aba::nova("x", "X", "X").grupo(Grupo::novo("g", "G", &["a", "b"]))];
        assert!(matches!(
            validar_abas(&ambigua, &lista2),
            Err(ErroAbas::KeyTipDeAcaoAmbiguo { .. })
        ));
        let repetida = vec![Aba::nova("x", "X", "A")];
        assert!(matches!(
            validar_abas(&repetida, &lista),
            Err(ErroAbas::KeyTipRepetido(_))
        ));
    }

    #[test]
    fn ctrl_f1_recolhe_e_volta_ao_modo_anterior() {
        let mut e = EstadoRibbon::default();
        e.definir_modo(ModoRibbon::Simples);
        e.alternar_recolhido();
        assert_eq!(e.modo, ModoRibbon::Recolhido);
        assert!(!e.mostra_corpo());
        e.alternar_recolhido();
        assert_eq!(e.modo, ModoRibbon::Simples);
        assert!(e.mostra_corpo());
    }

    #[test]
    fn recolhido_abre_o_corpo_por_cima_e_fecha_ao_executar() {
        let mut e = EstadoRibbon::default();
        e.alternar_recolhido();
        let t = Guia::Aba("triagem".into());
        e.clicar_guia(&t, false);
        assert!(e.mostra_corpo());
        e.apos_executar();
        assert!(!e.mostra_corpo());
        e.clicar_guia(&t, false);
        e.clicar_guia(&t, false);
        assert!(!e.mostra_corpo(), "segundo clique na mesma aba fecha");
        e.clicar_guia(&t, true);
        assert_eq!(e.modo, ModoRibbon::Classico, "duplo clique expande");
    }

    #[test]
    fn arquivo_abre_os_bastidores_e_aba_que_sumiu_cai_na_primeira() {
        let mut e = EstadoRibbon::default();
        e.clicar_guia(&Guia::Arquivo, false);
        assert!(e.bastidores_abertos && !e.mostra_corpo());
        e.clicar_guia(&Guia::Aba("achado".into()), false);
        assert!(!e.bastidores_abertos);
        let guias = guias_visiveis(&abas(), true);
        assert_eq!(e.aba_efetiva(&guias), Some("inicio"));
    }

    #[test]
    fn modo_simples_corta_da_direita_para_a_esquerda() {
        assert!(
            cortar(&[50.0, 50.0], 10.0, 200.0, 20.0)
                .excedentes
                .is_empty()
        );
        let c = cortar(&[50.0, 50.0, 50.0], 10.0, 150.0, 20.0);
        assert_eq!(c.visiveis, 2);
        assert_eq!(c.excedentes, vec![2]);
        let c = cortar(&[50.0, 50.0, 50.0], 10.0, 100.0, 30.0);
        assert_eq!(c.visiveis, 1);
    }

    #[test]
    fn keytips_alt_aba_e_acao() {
        let (abas, lista) = (abas(), acoes());
        let guias = guias_visiveis(&abas, true);
        let mut k = KeyTips::default();
        let t = |k: &mut KeyTips, e| k.tratar(e, &abas, &guias, &[], &lista);
        assert_eq!(t(&mut k, EntradaKeyTip::Letra('t')), ResultadoKeyTip::Nada);
        assert_eq!(t(&mut k, EntradaKeyTip::ToqueAlt), ResultadoKeyTip::Nada);
        assert_eq!(k.nivel(), &NivelKeyTip::Guias);
        assert_eq!(
            t(&mut k, EntradaKeyTip::Letra('t')),
            ResultadoKeyTip::AbrirAba("triagem".into())
        );
        assert_eq!(
            t(&mut k, EntradaKeyTip::Letra('x')),
            ResultadoKeyTip::Executar("descartar".into())
        );
        assert!(!k.ativo());
    }

    #[test]
    fn keytips_de_duas_letras_desabilitada_esc_e_arquivo() {
        let (abas, lista) = (abas(), acoes());
        let guias = guias_visiveis(&abas, true);
        let mut k = KeyTips::default();
        let t = |k: &mut KeyTips, e| k.tratar(e, &abas, &guias, &[], &lista);
        t(&mut k, EntradaKeyTip::ToqueAlt);
        t(&mut k, EntradaKeyTip::Letra('T'));
        assert_eq!(t(&mut k, EntradaKeyTip::Letra('U')), ResultadoKeyTip::Nada);
        assert_eq!(k.digitado(), "U");
        assert_eq!(
            t(&mut k, EntradaKeyTip::Letra('D')),
            ResultadoKeyTip::Executar("duvida".into())
        );
        t(&mut k, EntradaKeyTip::ToqueAlt);
        t(&mut k, EntradaKeyTip::Letra('T'));
        assert_eq!(
            t(&mut k, EntradaKeyTip::Letra('E')),
            ResultadoKeyTip::Nada,
            "desabilitada não dispara"
        );
        assert!(k.ativo());
        t(&mut k, EntradaKeyTip::Esc);
        assert_eq!(k.nivel(), &NivelKeyTip::Guias);
        assert_eq!(
            t(&mut k, EntradaKeyTip::Letra('a')),
            ResultadoKeyTip::AbrirBastidores
        );
        assert!(!k.ativo());
    }

    #[test]
    fn keytips_numeros_da_barra_de_acesso_rapido() {
        let (abas, lista) = (abas(), acoes());
        let guias = guias_visiveis(&abas, true);
        let rapido = vec!["doc".to_owned(), "enviar".to_owned()];
        let mut k = KeyTips::default();
        k.tratar(EntradaKeyTip::ToqueAlt, &abas, &guias, &rapido, &lista);
        assert_eq!(
            k.tratar(EntradaKeyTip::Letra('1'), &abas, &guias, &rapido, &lista),
            ResultadoKeyTip::Executar("doc".into())
        );
        k.tratar(EntradaKeyTip::ToqueAlt, &abas, &guias, &rapido, &lista);
        assert_eq!(
            k.tratar(EntradaKeyTip::Letra('2'), &abas, &guias, &rapido, &lista),
            ResultadoKeyTip::Nada
        );
    }
}
