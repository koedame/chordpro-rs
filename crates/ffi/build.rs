fn main() {
    uniffi::generate_scaffolding("src/chordsketch.udl")
        .expect("failed to generate UniFFI scaffolding from src/chordsketch.udl");
}
