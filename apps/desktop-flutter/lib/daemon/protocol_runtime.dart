// The small runtime protocol.g.dart is written against.

/// A value the generated code can put on the wire.
abstract interface class Wire {
  Object? toJson();
}

/// A protocol method: its name and how to read its result.
final class Method<P, R> {
  const Method(this.name, this.decodeResult);
  final String name;
  final R Function(Object? json) decodeResult;
}

/// A protocol notification: its name and how to read its params.
final class NotificationType<P> {
  const NotificationType(this.name, this.decodeParams);
  final String name;
  final P Function(Object? json) decodeParams;
}

/// [json] as a JSON object, or a [FormatException].
Map<String, Object?> asMap(Object? json) {
  if (json is Map<String, Object?>) return json;
  if (json is Map) return json.cast<String, Object?>();
  throw FormatException('expected a JSON object, got ${json.runtimeType}');
}

/// [json] as a JSON array, or a [FormatException].
List<Object?> asList(Object? json) {
  if (json is List<Object?>) return json;
  if (json is List) return json.cast<Object?>();
  throw FormatException('expected a JSON array, got ${json.runtimeType}');
}

/// Encodes params for the wire.
Object? encodeWire(Object? value) => value is Wire ? value.toJson() : value;
