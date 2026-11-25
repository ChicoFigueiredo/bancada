//! Verbos falsos que gravam as chamadas, para os testes do host Luau.
#![allow(dead_code)]

use std::collections::HashMap;
use std::time::Duration;

use bancada_navegador::luau::{ErroDeVerbo, ResultadoDeVerbo, Verbos};

/// Uma chamada ao trait, como o app a veria.
#[derive(Debug, Clone, PartialEq)]
pub enum Chamada {
    Abrir(String),
    Esperar(String, Duration),
    Existe(String),
    Clicar(String),
    Texto(String),
    Tabela(String),
    Preencher(String, String),
    PreencherSenha(String, String),
    PreencherCpf(String),
    PreencherCodigoDoEmail(String, Duration),
    Baixar(String, Option<String>),
    Pausa(String),
    Registrar(String),
    Hoje,
    SomarMeses(String, i32),
}

#[derive(Debug, Default)]
pub struct VerbosFalsos {
    pub chamadas: Vec<Chamada>,
    pub textos: HashMap<String, String>,
    pub tabelas: HashMap<String, Vec<Vec<String>>>,
    pub existentes: Vec<String>,
    pub esperar_resultado: Option<bool>,
    /// Erro devolvido pelo verbo de nome dado (ex.: "clicar"), uma vez.
    pub falhas: HashMap<&'static str, ErroDeVerbo>,
    pub arquivo_baixado: String,
    /// Pausa automática (CAPTCHA, código do aplicativo) simulada dentro de `abrir`, `clicar` e
    /// `esperar`: o verbo dorme esse tempo de verdade e o acumula para o host descontar.
    pub pausa_automatica: Duration,
    pendente: Duration,
    /// Trechos que `limpar` troca por `[segredo]`.
    pub segredos: Vec<String>,
}

impl VerbosFalsos {
    pub fn novo() -> Self {
        Self {
            arquivo_baixado: "contracheque-2026-11.pdf".into(),
            ..Self::default()
        }
    }

    pub fn falhando(mut self, verbo: &'static str, erro: ErroDeVerbo) -> Self {
        self.falhas.insert(verbo, erro);
        self
    }

    pub fn com_pausa_automatica(mut self, d: Duration) -> Self {
        self.pausa_automatica = d;
        self
    }

    pub fn com_segredo(mut self, s: &str) -> Self {
        self.segredos.push(s.into());
        self
    }

    fn pausar_sozinho(&mut self) {
        if !self.pausa_automatica.is_zero() {
            std::thread::sleep(self.pausa_automatica);
            self.pendente += self.pausa_automatica;
        }
    }

    fn falha(&mut self, verbo: &'static str) -> ResultadoDeVerbo<()> {
        match self.falhas.remove(verbo) {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

impl Verbos for VerbosFalsos {
    fn abrir(&mut self, url: &str) -> ResultadoDeVerbo<()> {
        self.chamadas.push(Chamada::Abrir(url.into()));
        self.pausar_sozinho();
        self.falha("abrir")
    }
    fn esperar(&mut self, seletor: &str, prazo: Duration) -> ResultadoDeVerbo<bool> {
        self.chamadas.push(Chamada::Esperar(seletor.into(), prazo));
        self.pausar_sozinho();
        self.falha("esperar")?;
        Ok(self.esperar_resultado.unwrap_or(true))
    }
    fn existe(&mut self, seletor: &str) -> ResultadoDeVerbo<bool> {
        self.chamadas.push(Chamada::Existe(seletor.into()));
        self.falha("existe")?;
        Ok(self.existentes.iter().any(|s| s == seletor))
    }
    fn clicar(&mut self, seletor: &str) -> ResultadoDeVerbo<()> {
        self.chamadas.push(Chamada::Clicar(seletor.into()));
        self.pausar_sozinho();
        self.falha("clicar")
    }
    fn texto(&mut self, seletor: &str) -> ResultadoDeVerbo<String> {
        self.chamadas.push(Chamada::Texto(seletor.into()));
        self.falha("texto")?;
        Ok(self.textos.get(seletor).cloned().unwrap_or_default())
    }
    fn tabela(&mut self, seletor: &str) -> ResultadoDeVerbo<Vec<Vec<String>>> {
        self.chamadas.push(Chamada::Tabela(seletor.into()));
        self.falha("tabela")?;
        Ok(self.tabelas.get(seletor).cloned().unwrap_or_default())
    }
    fn preencher(&mut self, seletor: &str, texto: &str) -> ResultadoDeVerbo<()> {
        self.chamadas
            .push(Chamada::Preencher(seletor.into(), texto.into()));
        self.falha("preencher")
    }
    fn preencher_senha(&mut self, seletor: &str, rotulo: &str) -> ResultadoDeVerbo<()> {
        self.chamadas
            .push(Chamada::PreencherSenha(seletor.into(), rotulo.into()));
        self.falha("preencher_senha")
    }
    fn preencher_cpf(&mut self, seletor: &str) -> ResultadoDeVerbo<()> {
        self.chamadas.push(Chamada::PreencherCpf(seletor.into()));
        self.falha("preencher_cpf")
    }
    fn preencher_codigo_do_email(
        &mut self,
        seletor: &str,
        janela: Duration,
    ) -> ResultadoDeVerbo<()> {
        self.chamadas
            .push(Chamada::PreencherCodigoDoEmail(seletor.into(), janela));
        self.falha("preencher_codigo_do_email")
    }
    fn baixar(&mut self, seletor: &str, nome: Option<&str>) -> ResultadoDeVerbo<String> {
        self.chamadas
            .push(Chamada::Baixar(seletor.into(), nome.map(Into::into)));
        self.falha("baixar")?;
        Ok(nome.map_or_else(|| self.arquivo_baixado.clone(), Into::into))
    }
    fn pausa(&mut self, motivo: &str) -> ResultadoDeVerbo<()> {
        self.chamadas.push(Chamada::Pausa(motivo.into()));
        self.falha("pausa")
    }
    fn registrar(&mut self, mensagem: &str) -> ResultadoDeVerbo<()> {
        self.chamadas.push(Chamada::Registrar(mensagem.into()));
        self.falha("registrar")
    }
    fn limpar(&self, texto: &str) -> String {
        let mut limpo = texto.to_owned();
        for s in &self.segredos {
            limpo = limpo.replace(s.as_str(), "[segredo]");
        }
        limpo
    }
    fn tempo_pausado_automatico(&mut self) -> Duration {
        std::mem::take(&mut self.pendente)
    }
    fn hoje(&mut self) -> ResultadoDeVerbo<String> {
        self.chamadas.push(Chamada::Hoje);
        Ok("2026-11-24".into())
    }
    fn somar_meses(&mut self, data: &str, meses: i32) -> ResultadoDeVerbo<String> {
        self.chamadas.push(Chamada::SomarMeses(data.into(), meses));
        Ok("2026-12-24".into())
    }
}
