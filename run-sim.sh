#!/bin/bash
JAR="data-generation/target/scala-2.13/forest-fire-simulation_2.13-1.0.jar" CP="$JAR:$(coursier fetch --classpath org.scala-lang:scala-library:2.13.12 com.typesafe.play::play-json:2.9.4)"

for arg in "$@"; do
  echo "Argument: $arg"
done

java -cp "$CP" Main "$@"
