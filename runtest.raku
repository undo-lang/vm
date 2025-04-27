#!/usr/bin/env raku
use Test;
use JSON::Fast;
use fatal;

my @specs = from-json(slurp 'test/run/spec.json');
for @specs -> % (:$name, :$is-error, :$skip, :@dependencies) {
  if $skip {
    skip "Skipping $name" ~ (" - $skip" if $skip ~~ Str);
    next;
  }

  my $bc-file = "test/run/$name.bc.json";
  my $expected-output = slurp("test/run/$name.output");
  my @output;
  my $error;

  @dependencies.=map({ "test/run/$_.bc.json" });
  my $proc = Proc::Async.new('cargo', 'run', $bc-file, |@dependencies);

  $proc.stdout.tap({ @output.push: $_ });
  $proc.stderr.tap({ $error ~= $_ });

  try {
    my $r = await $proc.start;

    if $is-error {
      if $r.exitcode {
        ok $error ~~ /$expected-output/, $name;
      } else {
        flunk "Expected error in $name";
      }
    } else {
      if $r.exitcode {
        diag "ERRORS:\n" ~ $error.lines.map("ERR: " ~ *).join("\n").indent(2);
        flunk $name;
      }

      is @output.join("\n"), $expected-output, $name;
    }
  }
}
