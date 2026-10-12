use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Nivel, Severidade};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArquivoDeCamada {
    pub nivel: Nivel,
    #[serde(default)]
    pub pisos: BTreeMap<String, Severidade>,
}

pub fn le_arquivo_de_camada(texto: &str) -> Result<ArquivoDeCamada, String> {
    serde_json::from_str(texto).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_file_as_the_flake_renders_it_reads() {
        let a = le_arquivo_de_camada(
            r#"{"nivel":"sistema","pisos":{"msg-placeholder-subject":"avisa","vcs-conflict-markers":"bloqueia"}}"#,
        )
        .expect("reads");
        assert_eq!(a.nivel, Nivel::Sistema);
        assert_eq!(a.pisos["vcs-conflict-markers"], Severidade::Bloqueia);
        assert_eq!(a.pisos["msg-placeholder-subject"], Severidade::Avisa);
    }

    #[test]
    fn the_user_file_as_the_flake_renders_it_reads() {
        let a = le_arquivo_de_camada(r#"{"nivel":"usuario","pisos":{}}"#).expect("reads");
        assert_eq!(a.nivel, Nivel::Usuario);
        assert!(a.pisos.is_empty());
    }

    #[test]
    fn the_english_names_the_trio_once_rendered_still_read() {
        let a = le_arquivo_de_camada(
            r#"{"nivel":"usuario","pisos":{"a":"advise","b":"warn-with-ratchet","c":"warn","d":"block"}}"#,
        )
        .expect("reads");
        let quer = [
            Severidade::Consultivo,
            Severidade::AvisaComCatraca,
            Severidade::Avisa,
            Severidade::Bloqueia,
        ];
        assert_eq!(a.pisos.values().copied().collect::<Vec<_>>(), quer);
    }

    #[test]
    fn every_symbol_round_trips_through_serde() {
        for s in Severidade::todos() {
            let j = serde_json::to_string(s).expect("serialises");
            assert_eq!(j, format!("\"{}\"", s.simbolo()));
        }
    }

    #[test]
    fn an_unknown_severity_is_refused() {
        assert!(le_arquivo_de_camada(r#"{"nivel":"usuario","pisos":{"a":"loud"}}"#).is_err());
    }
}
