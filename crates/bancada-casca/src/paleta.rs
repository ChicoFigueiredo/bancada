//! Paleta (Ctrl+K): busca aproximada sobre as Ações da lista única e sobre os destinos de
//! navegação que o app fornece. Modelo puro, porte do `palette.rs` do Chess Studio.

use crate::acoes::{IdAcao, ListaDeAcoes};
use crate::navegacao::{GrupoNavegacao, Rota};

/// Um destino de navegação oferecido na Paleta ("Fornecedor · Energia Exemplo").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Destino {
    pub rota: Rota,
    pub rotulo: String,
    pub icone: String,
    /// Texto curto à direita ("Fornecedor", "Área").
    pub tipo: String,
}

impl Destino {
    pub fn novo(rota: impl Into<Rota>, rotulo: impl Into<String>, tipo: impl Into<String>) -> Self {
        Self {
            rota: rota.into(),
            rotulo: rotulo.into(),
            icone: String::new(),
            tipo: tipo.into(),
        }
    }
}

/// O que uma linha da Paleta executa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Alvo {
    Acao(IdAcao),
    Destino(Rota),
}

/// Uma linha da Paleta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entrada {
    pub alvo: Alvo,
    pub rotulo: String,
    pub icone: String,
    /// Atalho (Ações) ou tipo (destinos), à direita.
    pub detalhe: String,
    pub motivo: Option<String>,
}

impl Entrada {
    pub fn habilitada(&self) -> bool {
        self.motivo.is_none()
    }
}

/// Estado da Paleta aberta.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EstadoPaleta {
    pub aberta: bool,
    pub busca: String,
    pub selecionada: usize,
}

impl EstadoPaleta {
    pub fn abrir(&mut self) {
        self.aberta = true;
        self.busca.clear();
        self.selecionada = 0;
    }

    pub fn fechar(&mut self) {
        self.aberta = false;
    }

    pub fn alternar(&mut self) {
        if self.aberta {
            self.fechar();
        } else {
            self.abrir();
        }
    }

    /// Seta para baixo/cima, presa ao tamanho da lista.
    pub fn mover(&mut self, delta: isize, total: usize) {
        if total == 0 {
            self.selecionada = 0;
            return;
        }
        let atual = self.selecionada.min(total - 1) as isize;
        self.selecionada = (atual + delta).clamp(0, total as isize - 1) as usize;
    }
}

/// Texto sem caixa nem acentos do pt-BR, para comparar ("Ação" → "acao").
pub fn normalizar(texto: &str) -> String {
    texto
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            outro => outro,
        })
        .collect()
}

/// Classe de um termo no rótulo: 0 = começa o rótulo, 1 = começa uma palavra, 2 = trecho no
/// meio de palavra, 3 = subsequência (as letras aparecem na ordem, espalhadas: "dcrt" acha
/// "Descartar"). `None` = não aparece.
fn classe(rotulo: &str, termo: &str) -> Option<u8> {
    if rotulo.starts_with(termo) {
        return Some(0);
    }
    let mut melhor = None;
    for (pos, _) in rotulo.match_indices(termo) {
        let inicio_de_palavra = rotulo[..pos]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric());
        if inicio_de_palavra {
            return Some(1);
        }
        melhor = Some(2);
    }
    if melhor.is_some() {
        return melhor;
    }
    let mut letras = rotulo.chars();
    termo.chars().all(|t| letras.any(|c| c == t)).then_some(3)
}

/// Índices dos rótulos que casam com a busca, do melhor para o pior. Sem busca, todos na ordem
/// original. Caixa e acentos não contam; cada palavra da busca precisa aparecer; a nota é a pior
/// das palavras, e empates mantêm a ordem original.
pub fn classificar<S: AsRef<str>>(busca: &str, rotulos: &[S]) -> Vec<usize> {
    let normalizada = normalizar(busca);
    let termos: Vec<&str> = normalizada.split_whitespace().collect();
    if termos.is_empty() {
        return (0..rotulos.len()).collect();
    }
    let mut notas: Vec<(u8, usize)> = rotulos
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            let r = normalizar(r.as_ref());
            let mut pior = 0;
            for t in &termos {
                pior = pior.max(classe(&r, t)?);
            }
            Some((pior, i))
        })
        .collect();
    notas.sort();
    notas.into_iter().map(|(_, i)| i).collect()
}

/// As linhas da Paleta: as Ações (menos as marcadas `fora_da_paleta`), depois os itens do
/// painel de navegação, depois os destinos extras do app. Destino repetido (mesma rota) entra
/// uma vez só.
pub fn entradas(
    acoes: &ListaDeAcoes,
    navegacao: &[GrupoNavegacao],
    destinos: &[Destino],
    tipo_area: &str,
) -> Vec<Entrada> {
    let mut saida: Vec<Entrada> = acoes
        .iter()
        .filter(|a| !a.fora_da_paleta)
        .map(|a| Entrada {
            alvo: Alvo::Acao(a.id.clone()),
            rotulo: a.rotulo.clone(),
            icone: a.icone.clone(),
            detalhe: a.atalho.map(|t| t.to_string()).unwrap_or_default(),
            motivo: a.motivo().map(str::to_owned),
        })
        .collect();
    let mut rotas = std::collections::HashSet::new();
    for item in navegacao.iter().flat_map(|g| &g.itens) {
        if rotas.insert(item.rota.clone()) {
            saida.push(Entrada {
                alvo: Alvo::Destino(item.rota.clone()),
                rotulo: item.rotulo.clone(),
                icone: item.icone.clone(),
                detalhe: tipo_area.to_owned(),
                motivo: item.indisponivel.as_ref().map(|i| i.motivo.clone()),
            });
        }
    }
    for d in destinos {
        if rotas.insert(d.rota.clone()) {
            saida.push(Entrada {
                alvo: Alvo::Destino(d.rota.clone()),
                rotulo: d.rotulo.clone(),
                icone: d.icone.clone(),
                detalhe: d.tipo.clone(),
                motivo: None,
            });
        }
    }
    saida
}

/// As linhas visíveis para a busca, em ordem.
pub fn visiveis(entradas: &[Entrada], busca: &str) -> Vec<usize> {
    let rotulos: Vec<&str> = entradas.iter().map(|e| e.rotulo.as_str()).collect();
    classificar(busca, &rotulos)
}

/// O que Enter (ou o clique) executa na linha escolhida: nada se está desabilitada.
pub fn ativar(entradas: &[Entrada], mostradas: &[usize], selecionada: usize) -> Option<Alvo> {
    let e = entradas.get(*mostradas.get(selecionada)?)?;
    e.habilitada().then(|| e.alvo.clone())
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::acoes::Acao;
    use crate::navegacao::ItemNavegacao;

    #[test]
    fn normaliza_caixa_e_acentos() {
        assert_eq!(normalizar("Ação Única"), "acao unica");
    }

    #[test]
    fn prefixo_vence_palavra_que_vence_trecho_que_vence_subsequencia() {
        let rotulos = [
            "Abrir descartados",
            "Descartar",
            "Indescartável",
            "Desfazer cartão",
        ];
        assert_eq!(classificar("descar", &rotulos), vec![1, 0, 2, 3]);
        assert_eq!(classificar("dcrt", &["Ver texto", "Descartar"]), vec![1]);
        assert_eq!(classificar("cart", &["Descartar", "Cartão"]), vec![1, 0]);
        assert_eq!(classificar("", &rotulos), vec![0, 1, 2, 3]);
        assert_eq!(classificar("desf cart", &rotulos), vec![3]);
        assert!(classificar("zzz", &rotulos).is_empty());
    }

    #[test]
    fn entradas_juntam_acoes_e_destinos_sem_repetir_rota() {
        let acoes = ListaDeAcoes::nova(vec![
            Acao::nova("a", "Descartar"),
            Acao::nova("p", "Paleta").fora_da_paleta(),
            Acao::nova("e", "Enviar").desabilitada("fatia 3"),
        ])
        .unwrap();
        let nav = vec![GrupoNavegacao::novo(
            "Inventário",
            vec![
                ItemNavegacao::novo("fornecedores", "Fornecedores", ""),
                ItemNavegacao::novo("hoje", "Hoje", "").indisponivel("fatia 2", "livro-caixa"),
            ],
        )];
        let destinos = vec![
            Destino::novo("fornecedores", "Fornecedores (repetido)", "Área"),
            Destino::novo("fornecedor:energia", "Energia Exemplo", "Fornecedor"),
        ];
        let e = entradas(&acoes, &nav, &destinos, "Área");
        let rotulos: Vec<_> = e.iter().map(|e| e.rotulo.as_str()).collect();
        assert_eq!(
            rotulos,
            vec![
                "Descartar",
                "Enviar",
                "Fornecedores",
                "Hoje",
                "Energia Exemplo"
            ]
        );
        let mostradas = visiveis(&e, "energia");
        assert_eq!(
            ativar(&e, &mostradas, 0),
            Some(Alvo::Destino(Rota::nova("fornecedor:energia")))
        );
        let mostradas = visiveis(&e, "enviar");
        assert_eq!(ativar(&e, &mostradas, 0), None, "desabilitada não executa");
        let mostradas = visiveis(&e, "hoje");
        assert_eq!(
            ativar(&e, &mostradas, 0),
            None,
            "área indisponível não abre"
        );
    }

    #[test]
    fn mover_a_selecao_fica_dentro_da_lista() {
        let mut p = EstadoPaleta::default();
        p.abrir();
        p.mover(-1, 3);
        assert_eq!(p.selecionada, 0);
        p.mover(5, 3);
        assert_eq!(p.selecionada, 2);
        p.mover(1, 0);
        assert_eq!(p.selecionada, 0);
    }
}
