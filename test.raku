use Test;

for dir('test/run/') -> $bc-file {
  next unless $bc-file ~~ /'.bc.json'$/;
  my $expected-output = slurp($bc-file.subst(/'.bc.json'$/, '.output'));
  my @output;
  my $proc = Proc::Async.new(<<cargo run $bc-file>>);
  $proc.stdout.tap({ @output.push: $_ });
  $proc.stderr.tap({ $_ });
  await $proc.start;

  is @output.join("\n"), $expected-output, "$bc-file";
}
