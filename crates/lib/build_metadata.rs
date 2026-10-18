// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Qualification inputs shared by the build script and its direct tests.
pub(crate) fn qualification<'a>(
    kind: Option<&str>,
    revision: Option<&'a str>,
) -> Result<(&'static str, Option<&'a str>), String> {
    match (kind,revision){
        (None,None)|(Some("local"),None)=>Ok(("local",None)),
        (Some("candidate"|"release"),Some(revision)) if revision.len()==40&&revision.bytes().all(|byte|byte.is_ascii_hexdigit())=>Ok((if kind==Some("candidate"){"candidate"}else{"release"},Some(revision))),
        (None,Some(_))=>Err("AUDITORY_SOURCE_REVISION requires AUDITORY_BUILD_KIND=candidate or release; unset both for an unqualified local build".into()),
        (Some("local"),Some(_))=>Err("local builds cannot claim AUDITORY_SOURCE_REVISION; unset the revision".into()),
        (Some("candidate"|"release"),_)=>Err("qualified builds require AUDITORY_SOURCE_REVISION containing exactly 40 hexadecimal characters".into()),
        _=>Err("AUDITORY_BUILD_KIND must be local, candidate, or release; unset identity inputs for an unqualified local build".into()),
    }
}
