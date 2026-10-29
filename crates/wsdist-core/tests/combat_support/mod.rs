use std::path::{Path,PathBuf};
use std::sync::Arc;
use serde::Deserialize;
use serde_json::Value;
use wsdist_core::{Catalog,build_player};
use wsdist_core::actions::{evaluate_action,resolve_spell_name,sample_round,sample_ws};
use wsdist_core::model::{ActionRequest,Abilities,CalcError,Enemy,EvaluationContext,HitEvent,LegacyF64,Metric,Player,RandomSource,RandomStream,SkillType,Slot,SpellRequest,SpellType,WsRequest,WsType};
use wsdist_core::rng::{RandomSources,ScriptedTape};
use wsdist_core::weaponskills::resolve_name;

#[path="../legacy_player/support.rs"]
#[allow(dead_code)]
mod player_support;
pub use player_support::observation;

#[derive(Deserialize)]
pub struct IndexedCase {pub id:String,pub operation:String,pub path:String}
#[derive(Deserialize)]
pub struct FixtureIndex {pub cases:Vec<IndexedCase>}
pub fn resource_root()->PathBuf {Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")}
pub fn fixture_root()->PathBuf {Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/legacy")}
pub fn document(path:&Path)->Value {player_support::document(path)}
pub fn index(root:&Path)->FixtureIndex {serde_json::from_value(document(&root.join("combat.json"))).unwrap()}
pub fn number(value:&Value)->f64 {serde_json::from_value::<LegacyF64>(value.clone()).expect("captured number").0}

#[derive(Default)]
pub struct ReplayCounts {pub compiled:usize,pub scripted:usize,pub hits:usize,pub uniform_draws:usize,pub legacy_errors:usize,pub last_fields:Option<Value>,pub scripted_trace:Option<Vec<HitEvent>>}

#[derive(Clone,Debug)]
struct UniformDraw {stream:RandomStream,low:f64,high:f64,value:f64}
#[derive(Clone)]
struct Recorded<R> {inner:R,draws:Vec<UniformDraw>}
impl<R:RandomSource> RandomSource for Recorded<R> {
    fn uniform(&mut self,stream:RandomStream,low:f64,high:f64)->f64 {
        let value=self.inner.uniform(stream,low,high);
        self.draws.push(UniformDraw{stream,low,high,value});value
    }
    fn bounded_numpy(&mut self,upper_exclusive:usize)->usize {panic!("combat must not choose: bound {upper_exclusive}")}
    fn reseed_numpy(&mut self,seed:u32) {panic!("combat must not reseed: {seed}")}
}

fn request(case:&Value)->Result<ActionRequest,CalcError> {
    let input=&case["input"]["action"];
    let metric=Metric::new(input["metric"].as_str().expect("original metric"));
    match case["operation"].as_str().unwrap() {
        "combat.round"=>Ok(ActionRequest::Round{metric,starting_tp:number(&input["starting_tp"]),threshold:number(&input["ws_threshold"])}),
        "combat.ws"=>Ok(ActionRequest::Ws(WsRequest{skill:resolve_name(input["name"].as_str().unwrap())?,ws_type:WsType::from_name(input["ws_type"].as_str().unwrap()).unwrap(),tp:number(&input["tp"]),metric})),
        "combat.spell"=>Ok(ActionRequest::Spell(SpellRequest{spell:resolve_spell_name(input["name"].as_str().unwrap())?,spell_type:SpellType::from_name(input["spell_type"].as_str().unwrap()).unwrap(),metric})),
        operation=>panic!("not a combat action: {operation}"),
    }
}

pub fn build_boundary<R:RandomSource>(case:&Value,ctx:&mut EvaluationContext<R>)->Result<Player,CalcError> {
    let label=case["id"].as_str().unwrap();
    let input=player_support::parse_input(&case["input"]["player"],&ctx.catalog,&ctx.state,label);
    let mut player=Player::new(input.gearset);
    build_player(&input,ctx,&mut player)?;
    for (name,value) in case["input"]["player"]["stat_overrides"].as_object().unwrap() {
        player.stats.set(wsdist_core::Stat::from_name(name).unwrap_or_else(||panic!("{label}: unknown override {name}")),number(value));
    }
    let boundary=&case["expected"]["value"]["player_at_action_boundary"];
    player_support::assert_metadata(&player,boundary,label);
    player_support::assert_sheet(&player.stats,&player.wsc,player.wyvern_bonus_attack,boundary,label);
    assert_eq!(serde_json::to_value(&player.abilities).unwrap(),serde_json::to_value(serde_json::from_value::<Abilities>(boundary["abilities"].clone()).unwrap()).unwrap(),"{label}: effective ability state");
    for slot in Slot::ALL {
        let piece=ctx.catalog.piece(player.gearset[slot.index()]);
        let captured=&boundary["gearset"][slot.name()];
        player_support::assert_piece(piece,Some(&ctx.state),captured,&format!("{label}.{}",slot.name()));
        let skill=SkillType::from_name(captured["skill_type"].as_str().unwrap()).unwrap();
        assert_eq!(ctx.state.skill(piece),skill,"{label}: effective original-identity skill");
    }
    Ok(player)
}

pub fn boundary_enemy(case:&Value)->Enemy {
    Enemy{name:Arc::from("captured combat enemy"),location:Arc::from("captured"),stats:serde_json::from_value(case["expected"]["value"]["enemy_at_action_boundary"].clone()).unwrap()}
}

fn evaluate<R:RandomSource>(case:&Value,player:&Player,enemy:&Enemy,ctx:&mut EvaluationContext<R>)->Result<Value,CalcError> {
    let action=request(case)?;
    if case["input"]["action"]["simulation"]==true {
        match &action {
            ActionRequest::Round{starting_tp,threshold,..}=>{
                let result=sample_round(player,enemy,*starting_tp,*threshold,ctx)?;
                Ok(serde_json::to_value(result).unwrap())
            }
            ActionRequest::Ws(request)=>Ok(serde_json::to_value(sample_ws(player,enemy,request,case["input"]["action"]["single"].as_bool().unwrap(),ctx)?).unwrap()),
            ActionRequest::Spell(_)=>panic!("source spells have no sampled return"),
        }
    } else {
        let result=evaluate_action(player,enemy,&action,ctx)?;
        let mut result=serde_json::to_value(result).unwrap();
        result["seconds_per_ws"]=result["time_to_ws"].clone();
        Ok(result)
    }
}

fn assert_error(error:&CalcError,expected:&Value,label:&str) {
    let CalcError::Legacy{category,message,..}=error else {panic!("{label}: expected source error, got {error}")};
    assert_eq!(*category,expected["class"].as_str().expect("source exception class"),"{label}: exception category");
    assert_eq!(message,expected["message"].as_str().unwrap(),"{label}: exception message");
}

fn assert_fields(result:Result<Value,CalcError>,execution:&Value,label:&str)->Option<Value> {
    if execution["status"]=="legacy_error" {
        let error=result.expect_err(&format!("{label}: expected source error"));
        assert_error(&error,&execution["error"],label);None
    } else {
        let actual=result.unwrap_or_else(|error|panic!("{label}: {error}"));
        for (name,value) in execution["fields"].as_object().unwrap() {
            if value.is_null() {assert!(actual[name].is_null(),"{label}.{name}: captured absence");}
            else {player_support::assert_number(number(&actual[name]),value,&format!("{label}.{name}"));}
        }
        Some(actual)
    }
}

fn assert_draws(actual:&[UniformDraw],expected:&Value,label:&str) {
    let expected=expected.as_array().unwrap();
    assert_eq!(actual.len(),expected.len(),"{label}: complete random draw consumption");
    for (index,(actual,expected)) in actual.iter().zip(expected).enumerate() {
        let stream=match expected["stream"].as_str().unwrap() {"NumpyHost"=>RandomStream::NumpyHost,"NumbaPython"=>RandomStream::NumbaPython,other=>panic!("unexpected stream {other}")};
        assert_eq!(actual.stream,stream,"{label}.draw[{index}]: stream/order");
        assert_eq!(expected["operation"],"uniform","{label}.draw[{index}]: operation");
        assert_eq!(actual.low.to_bits(),number(&expected["parameters"][0]).to_bits(),"{label}.draw[{index}]: low parameter bits");
        assert_eq!(actual.high.to_bits(),number(&expected["parameters"][1]).to_bits(),"{label}.draw[{index}]: high parameter bits");
        assert_eq!(actual.value.to_bits(),number(&expected["value"]).to_bits(),"{label}.draw[{index}]: exact variate");
    }
}

fn assert_stream_snapshot(random:&RandomSources,checkpoint:&Value,label:&str) {
    let mut cloned=random.clone();
    for (stream,key) in [(RandomStream::NumpyHost,"numpy_host"),(RandomStream::NumbaPython,"numba_python")] {
        for (index,expected) in checkpoint[key]["next_uniforms_from_independent_clone"].as_array().expect("captured independent MT continuation").iter().enumerate() {
            assert_eq!(cloned.random53(stream).to_bits(),number(expected).to_bits(),"{label}.{key}.next[{index}]: full stream continuation");
        }
    }
}

pub fn replay_case(case:&Value,catalog:&Arc<Catalog>)->ReplayCounts {
    assert_eq!(case["expected"]["outcome"],"result","source capture protocol failed");
    let label=case["id"].as_str().unwrap();
    let random=&case["input"]["random"];
    let source=RandomSources::from_seeds(random["numpy_host_seed"].as_u64().unwrap() as u32,random["numba_python_seed"].as_u64().unwrap() as u32);
    let mut ctx=EvaluationContext::new(Arc::clone(catalog),Recorded{inner:source,draws:Vec::new()});
    let expected=&case["expected"]["value"];
    if expected["status"]=="legacy_error" {
        let input=player_support::parse_input(&case["input"]["player"],&ctx.catalog,&ctx.state,label);
        let mut player=Player::new(input.gearset);
        assert_error(&build_player(&input,&mut ctx,&mut player).expect_err("captured build rejection"),&expected["error"],label);
        assert!(ctx.random.draws.is_empty(),"{label}: rejected build consumed RNG");
        return ReplayCounts{legacy_errors:1,..ReplayCounts::default()};
    }
    let player=build_boundary(case,&mut ctx).unwrap_or_else(|error|panic!("{label}: build failed: {error}"));
    assert!(ctx.random.draws.is_empty(),"{label}: player build must not consume RNG");
    assert_stream_snapshot(&ctx.random.inner,&expected["compiled"]["rng_before"],&format!("{label}.before"));
    let enemy=boundary_enemy(case);
    ctx.action_trace=Some(Vec::with_capacity(16));
    let result=evaluate(case,&player,&enemy,&mut ctx);
    let last_fields=assert_fields(result,&expected["compiled"],label);
    assert_draws(&ctx.random.draws,&expected["compiled"]["draws"],label);
    assert_stream_snapshot(&ctx.random.inner,&expected["compiled"]["rng_after"],&format!("{label}.after"));
    let mut counts=ReplayCounts{compiled:1,scripted:0,hits:ctx.action_trace.as_ref().unwrap().len(),uniform_draws:ctx.random.draws.len(),legacy_errors:usize::from(last_fields.is_none()),last_fields,scripted_trace:None};
    if let Some(scripted)=expected.get("scripted") {
        let tape=ScriptedTape::from_json_records(&scripted["tape"]).expect("captured strict tape");
        let mut scripted_ctx=EvaluationContext::new(Arc::clone(catalog),Recorded{inner:tape,draws:Vec::new()});
        scripted_ctx.state=ctx.state.clone();
        scripted_ctx.action_trace=Some(Vec::with_capacity(16));
        let result=evaluate(case,&player,&enemy,&mut scripted_ctx);
        counts.last_fields=assert_fields(result,scripted,label);
        counts.uniform_draws+=scripted_ctx.random.draws.len();
        counts.legacy_errors+=usize::from(counts.last_fields.is_none());
        assert_draws(&scripted_ctx.random.draws,&scripted["tape"],label);
        scripted_ctx.random.inner.assert_finished().expect("no unused scripted draws");
        counts.scripted_trace=scripted_ctx.action_trace.take();
        counts.scripted=1;counts.hits+=counts.scripted_trace.as_ref().unwrap().len();
    }
    counts
}
