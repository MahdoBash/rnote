#[path = "../crates/rnote-engine/src/pens/brush/recognize.rs"] mod new;
#[path = "old-recognize.rs"] mod old;
use std::f64::consts::TAU;
type P = (f64,f64);
fn interp(a:P,b:P,t:f64)->P {(a.0+t*(b.0-a.0),a.1+t*(b.1-a.1))}
fn on_edges(v:&[P], t:f64, closed:bool)->P {
 let k=if closed {v.len()}else{v.len()-1};
 let t= if closed {t.rem_euclid(1.)}else{t.clamp(0.,1.)};
 let i=((t*k as f64).floor() as usize).min(k-1);
 interp(v[i],v[(i+1)%v.len()],t*k as f64-i as f64)
}
fn samples(kind:&str, seed:usize)->Vec<P> {
 let n=match kind {"triangle"=>3,"pentagon"=>5,"hexagon"=>6,"octagon"=>8,_=>4};
 let closed=!matches!(kind,"line"|"arrow"|"open U"|"zigzag");
 let polygon:Vec<P>=match kind {
 "rect"=>vec![(-1.,-0.55),(1.,-0.55),(1.,0.55),(-1.,0.55)],
 "arrow"=>vec![(-1.,0.),(1.,0.),(0.58,-0.29),(0.97,0.03),(0.59,0.32)],
 "open U"=>vec![(-1.,-0.7),(-1.,0.7),(1.,0.7),(1.,-0.7)],
 "zigzag"=>vec![(-1.,-0.7),(-0.4,0.7),(0.,-0.7),(0.6,0.7),(1.,-0.3)],
 _=>(0..n).map(|i|{let a=TAU*i as f64/n as f64;(a.cos(),a.sin())}).collect(),
 };
 let size=[27.,40.,59.,85.][seed%4];
 let (s,c)=(0.31*seed as f64).sin_cos();
 let num=[34,62,97,131,53][seed%5];
 let amp=[0.55,0.95,1.55][(seed/3)%3];
 let direction=if seed%2==0{1.}else{-1.};
 let offset=if closed {(0.071*seed as f64).fract()}else{0.};
 (0..=num).map(|i|{
   let frac=(i as f64/num as f64).powf([0.9,1.1,1.45][seed%3]);
   let t=if closed {offset+direction*frac*[1.,0.975,1.015][(seed/4)%3]}else if direction<0. {1.-frac}else{frac};
   let (x,y)=match kind {
    "circle"=>((TAU*t).cos(),(TAU*t).sin()),
    "ellipse"=>((TAU*t).cos(),0.45*(TAU*t).sin()),
    "line"=>(2.*t-1.,0.),
    "figure 8"=>((TAU*t).sin(),0.65*(2.*TAU*t).sin()),
    _=>on_edges(&polygon,t,closed),
   };
   let j=seed as f64;
   let dx=amp*((11.*t+j).sin()+0.35*(29.*t+0.8*j).cos());
   let dy=amp*((17.*t-j).cos()+0.35*(31.*t+0.4*j).sin());
   (280.+size*(c*x-s*y)+dx, 220.+size*(s*x+c*y)+dy)
 }).collect()
}
fn prediction(v:&[(f64,f64)])->String{
 if v.is_empty(){return "none".into();}
 if v.len()==2{return "line".into();}
 if v.len()==97{return "round".into();}
 if v.len()==5 && (v[0].0-v[4].0).hypot(v[0].1-v[4].1)>2.{return "arrow".into();}
 format!("p{}",v.len()-1)
}
fn main(){
 for (kind,want) in [("line","line"),("arrow","arrow"),("rect","p4"),("triangle","p3"),("pentagon","p5"),("hexagon","p6"),("octagon","p8"),("circle","round"),("ellipse","round"),("open U","none"),("zigzag","none"),("figure 8","none")] {
  let mut pass=[0;2]; let mut misses=std::collections::BTreeMap::new();
  for seed in 0..60{
   let pts=samples(kind,seed);
   let p1:Vec<_>=pts.iter().map(|p|old::Point(p.0,p.1)).collect();
   let p2:Vec<_>=pts.iter().map(|p|new::Point(p.0,p.1)).collect();
   let q1:Vec<_>=old::recognize(&p1).unwrap_or_default().into_iter().map(|p|(p.0,p.1)).collect();
   let q2:Vec<_>=new::recognize(&p2).unwrap_or_default().into_iter().map(|p|(p.0,p.1)).collect();
   pass[0]+=(prediction(&q1)==want) as usize; let got=prediction(&q2); pass[1]+=(got==want) as usize;
   if got!=want{*misses.entry(got).or_insert(0)+=1;}
  }
  println!("CORPUS {kind}: {}/60 -> {}/60 {misses:?}",pass[0],pass[1]);
 }
}
