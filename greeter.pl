#!/usr/bin/perl

$|++;

print "Enter your name:\n";

while(<STDIN>) {
   chomp;
   print "Hello, $_!\n";
   exit();
}
