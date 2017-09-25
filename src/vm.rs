enum Expr {
  Int(i64),
  Str(String)
}

#[derive(Serialize, Deserialize)]
enum Instruction {

}

struct VM<'a> {
  instructions: Vec<Instruction>
}

pub fn run() {
  println!("hey");
}
