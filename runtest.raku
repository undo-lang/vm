#!/usr/bin/env raku
use Test;
use JSON::Fast;

for dir('test/run/') -> $spec-file {
  next unless $spec-file ~~ /'.spec.json'/;
  slurp($spec-file);
}

my @specs = from-json(slurp 'test/run/spec.json');
for @specs -> % (:$name, :$is-error) {
  my $bc-file = "test/run/$($name).";
  next unless $bc-file ~~ /'.bc.json'$/;
  my $expected-output = slurp($bc-file.subst(/'.bc.json'$/, '.output'));
  my @output;
  my $error;
  my $proc = Proc::Async.new(<<cargo run $bc-file>>);
  $proc.stdout.tap({ @output.push: $_ });
  $proc.stderr.tap({ $error ~= $_ });
  try {
    await $proc.start;

    is @output.join("\n"), $expected-output, "$bc-file";
    CATCH {
      say "ERRORS:\n" ~ $error.lines.map("ERR: " ~ *).join("\n").indent(2) if $error;
    }
  }
}
