//! Métadonnées de langage du LSP Kastel.
//!
//! Cette table est volontairement alignée sur le lexer, le système de types
//! et les dispatchers réels de `kastel`.

/// Mots-clés réservés par `Token::keyword`.
pub const KEYWORDS: &[&str] = &[
    "let",
    "const",
    "func",
    "return",
    "if",
    "else",
    "while",
    "for",
    "in",
    "match",
    "true",
    "false",
    "None",
    "break",
    "continue",
    "import",
    "from",
    "as",
    "export",
    "class",
    "new",
    "this",
    "base",
    "interface",
    "try",
    "catch",
    "throw",
    "finally",
    "is",
];

/// Types disponibles dans la syntaxe Kastel.
pub const TYPE_NAMES: &[&str] = &[
    "int", "float", "str", "bool", "None", "List", "Dict", "Tuple", "Set", "Range", "dynamic",
    "any",
];

/// Mots contextuels utilisés par les membres de classe et les alias.
pub const CONTEXTUAL_KEYWORDS: &[&str] = &["public", "private", "type"];

/// Fonctions natives globales réellement exposées par le runtime.
/// `(nom, signature, documentation)`.
pub const BUILTIN_FUNCTIONS: &[(&str, &str, &str)] = &[
    (
        "print",
        "print(value)",
        "Affiche une valeur sans retour à la ligne.",
    ),
    (
        "println",
        "println(value)",
        "Affiche une valeur suivie d'un retour à la ligne.",
    ),
    (
        "input",
        "input(prompt?)",
        "Lit une ligne depuis l'entrée standard.",
    ),
    ("int", "int(value)", "Convertit une valeur en entier."),
    ("float", "float(value)", "Convertit une valeur en flottant."),
    ("str", "str(value)", "Convertit une valeur en chaîne."),
    ("bool", "bool(value)", "Convertit une valeur en booléen."),
    (
        "type",
        "type(value)",
        "Renvoie le nom du type d'une valeur.",
    ),
    ("clock", "clock()", "Horodatage courant."),
    ("cwd", "cwd()", "Répertoire de travail courant."),
    ("env", "env(name)", "Lit une variable d'environnement."),
    ("rand", "rand()", "Nombre flottant aléatoire."),
    (
        "rand_int",
        "rand_int(max)",
        "Entier aléatoire dans [0, max).",
    ),
    (
        "rand_range",
        "rand_range(low, high)",
        "Entier aléatoire dans [low, high).",
    ),
    ("abs", "abs(x)", "Valeur absolue."),
    ("floor", "floor(x)", "Arrondi vers -∞."),
    ("ceil", "ceil(x)", "Arrondi vers +∞."),
    ("round", "round(x)", "Arrondi au plus proche."),
    ("sqrt", "sqrt(x)", "Racine carrée."),
    ("pow", "pow(base, exp)", "Puissance."),
    ("min", "min(a, b)", "Minimum de deux valeurs."),
    ("max", "max(a, b)", "Maximum de deux valeurs."),
    ("sin", "sin(x)", "Sinus, en radians."),
    ("cos", "cos(x)", "Cosinus, en radians."),
    ("tan", "tan(x)", "Tangente, en radians."),
    ("asin", "asin(x)", "Arc sinus, en radians."),
    ("acos", "acos(x)", "Arc cosinus, en radians."),
    ("atan", "atan(x)", "Arc tangente, en radians."),
    ("atan2", "atan2(y, x)", "Arc tangente à deux arguments."),
    ("log", "log(x)", "Logarithme naturel."),
    ("log10", "log10(x)", "Logarithme base 10."),
    ("exp", "exp(x)", "Exponentielle."),
    ("idiv", "idiv(a, b)", "Division entière arrondie vers -∞."),
    (
        "wrapping_add",
        "wrapping_add(a, b)",
        "Addition entière modulo 2^64.",
    ),
    (
        "wrapping_sub",
        "wrapping_sub(a, b)",
        "Soustraction entière modulo 2^64.",
    ),
    (
        "wrapping_mul",
        "wrapping_mul(a, b)",
        "Multiplication entière modulo 2^64.",
    ),
    ("dict", "dict()", "Crée un dictionnaire vide."),
    ("list", "list(iterable)", "Convertit un itérable en List."),
    (
        "range",
        "range(stop) / range(start, stop) / range(start, stop, step)",
        "Crée une séquence entière.",
    ),
    (
        "format",
        "format(template, ...args)",
        "Formate une chaîne avec des valeurs.",
    ),
    (
        "inspect",
        "inspect(value)",
        "Représentation détaillée sur stderr.",
    ),
    ("debug", "debug(...)", "Trace de débogage sur stderr."),
    (
        "json_encode",
        "json_encode(value)",
        "Encode une valeur en JSON.",
    ),
    (
        "json_decode",
        "json_decode(text)",
        "Décode une chaîne JSON.",
    ),
    ("file_read", "file_read(path)", "Lit tout un fichier."),
    (
        "file_read_lines",
        "file_read_lines(path)",
        "Lit les lignes d'un fichier.",
    ),
    (
        "file_write",
        "file_write(path, content)",
        "Écrit un fichier.",
    ),
    (
        "file_append",
        "file_append(path, content)",
        "Ajoute au fichier.",
    ),
    (
        "file_exists",
        "file_exists(path)",
        "Teste l'existence d'un fichier.",
    ),
    ("file_delete", "file_delete(path)", "Supprime un fichier."),
    (
        "file_size",
        "file_size(path)",
        "Renvoie la taille d'un fichier.",
    ),
    (
        "path_join",
        "path_join(parts)",
        "Assemble des segments de chemin.",
    ),
    (
        "path_exists",
        "path_exists(path)",
        "Teste l'existence d'un chemin.",
    ),
    (
        "path_is_dir",
        "path_is_dir(path)",
        "Teste si le chemin est un dossier.",
    ),
    (
        "path_is_file",
        "path_is_file(path)",
        "Teste si le chemin est un fichier.",
    ),
    (
        "path_absolute",
        "path_absolute(path)",
        "Renvoie un chemin absolu.",
    ),
    (
        "path_basename",
        "path_basename(path)",
        "Extrait le nom de fichier.",
    ),
    (
        "path_dirname",
        "path_dirname(path)",
        "Extrait le dossier parent.",
    ),
    (
        "path_extension",
        "path_extension(path)",
        "Extrait l'extension.",
    ),
    ("path_stem", "path_stem(path)", "Extrait le stem."),
    ("os_name", "os_name()", "Nom du système d'exploitation."),
    ("os_arch", "os_arch()", "Architecture de la machine."),
    ("args", "args()", "Arguments du processus courant."),
    ("exit", "exit(code)", "Termine le processus."),
    ("Set", "Set(...values)", "Crée un ensemble Set."),
];

/// Noms reconnus par l'analyse sémantique comme builtins.
pub const BUILTINS: &[&str] = &[
    "print",
    "println",
    "input",
    "int",
    "float",
    "str",
    "bool",
    "type",
    "clock",
    "cwd",
    "env",
    "rand",
    "rand_int",
    "rand_range",
    "abs",
    "floor",
    "ceil",
    "round",
    "sqrt",
    "pow",
    "min",
    "max",
    "sin",
    "cos",
    "tan",
    "asin",
    "acos",
    "atan",
    "atan2",
    "log",
    "log10",
    "exp",
    "idiv",
    "wrapping_add",
    "wrapping_sub",
    "wrapping_mul",
    "dict",
    "list",
    "range",
    "format",
    "inspect",
    "debug",
    "json_encode",
    "json_decode",
    "file_read",
    "file_read_lines",
    "file_write",
    "file_append",
    "file_exists",
    "file_delete",
    "file_size",
    "path_join",
    "path_exists",
    "path_is_dir",
    "path_is_file",
    "path_absolute",
    "path_basename",
    "path_dirname",
    "path_extension",
    "path_stem",
    "os_name",
    "os_arch",
    "args",
    "exit",
    "Set",
];

/// Méthodes du conteneur syntaxique `List<T>`.
pub const LIST_METHODS: &[(&str, &str, &str)] = &[
    ("size", "value.size() -> int", "Nombre d'éléments."),
    (
        "is_empty",
        "value.is_empty() -> bool",
        "Indique si la liste est vide.",
    ),
    (
        "add",
        "value.add(element) -> bool",
        "Ajoute un élément à la fin.",
    ),
    (
        "remove",
        "value.remove(element) -> bool",
        "Supprime la première occurrence.",
    ),
    (
        "remove_at",
        "value.remove_at(index) -> T",
        "Supprime l'élément à l'index.",
    ),
    (
        "to_string",
        "value.to_string() -> str",
        "Convertit la liste en chaîne.",
    ),
    (
        "pop",
        "value.pop() -> T",
        "Retire et renvoie le dernier élément.",
    ),
    (
        "insert",
        "value.insert(index, element)",
        "Insère un élément à l'index.",
    ),
    ("get", "value.get(index) -> T", "Lit un élément."),
    ("set", "value.set(index, element)", "Remplace un élément."),
    (
        "contains",
        "value.contains(element) -> bool",
        "Teste l'appartenance.",
    ),
    (
        "index_of",
        "value.index_of(element) -> int",
        "Renvoie le premier index.",
    ),
    (
        "slice",
        "value.slice(start, end) -> List<T>",
        "Extrait une tranche.",
    ),
    ("reverse", "value.reverse()", "Inverse la liste."),
    ("join", "value.join(separator)", "Concatène les éléments."),
    ("clear", "value.clear()", "Vide la liste."),
    ("first", "value.first() -> T", "Premier élément."),
    ("last", "value.last() -> T", "Dernier élément."),
    ("sort", "value.sort()", "Trie la liste."),
    ("copy", "value.copy() -> List<T>", "Copie superficielle."),
    ("iter", "value.iter()", "Renvoie un itérateur."),
];

/// Méthodes réelles de `str`.
pub const STRING_METHODS: &[(&str, &str, &str)] = &[
    ("size", "value.size() -> int", "Nombre de caractères."),
    (
        "is_empty",
        "value.is_empty() -> bool",
        "Indique si la chaîne est vide.",
    ),
    (
        "to_string",
        "value.to_string() -> str",
        "Renvoie la chaîne elle-même.",
    ),
    ("get", "value.get(index) -> str", "Caractère à l'index."),
    (
        "contains",
        "value.contains(needle) -> bool",
        "Teste la présence d'une sous-chaîne.",
    ),
    (
        "starts_with",
        "value.starts_with(prefix) -> bool",
        "Teste le préfixe.",
    ),
    (
        "ends_with",
        "value.ends_with(suffix) -> bool",
        "Teste le suffixe.",
    ),
    (
        "index_of",
        "value.index_of(needle) -> int",
        "Premier index d'une sous-chaîne.",
    ),
    (
        "last_index_of",
        "value.last_index_of(needle) -> int",
        "Dernier index d'une sous-chaîne.",
    ),
    (
        "slice",
        "value.slice(start, end) -> str",
        "Extrait une tranche.",
    ),
    (
        "substring",
        "value.substring(start, length) -> str",
        "Extrait une sous-chaîne.",
    ),
    ("upper", "value.upper() -> str", "Convertit en majuscules."),
    ("lower", "value.lower() -> str", "Convertit en minuscules."),
    (
        "trim",
        "value.trim() -> str",
        "Supprime les espaces autour.",
    ),
    (
        "trim_start",
        "value.trim_start() -> str",
        "Supprime les espaces au début.",
    ),
    (
        "trim_end",
        "value.trim_end() -> str",
        "Supprime les espaces à la fin.",
    ),
    (
        "replace",
        "value.replace(from, to) -> str",
        "Remplace une occurrence.",
    ),
    (
        "replace_all",
        "value.replace_all(from, to) -> str",
        "Remplace toutes les occurrences.",
    ),
    (
        "split",
        "value.split(separator) -> List<str>",
        "Découpe la chaîne.",
    ),
    (
        "join",
        "value.join(list) -> str",
        "Utilise la chaîne comme séparateur.",
    ),
    ("repeat", "value.repeat(count) -> str", "Répète la chaîne."),
    (
        "char_at",
        "value.char_at(index) -> str",
        "Lit un caractère.",
    ),
    ("to_int", "value.to_int() -> int", "Convertit vers int."),
    (
        "to_float",
        "value.to_float() -> float",
        "Convertit vers float.",
    ),
    (
        "is_digit",
        "value.is_digit() -> bool",
        "Teste si tous les caractères sont numériques.",
    ),
    (
        "is_alpha",
        "value.is_alpha() -> bool",
        "Teste si tous les caractères sont alphabétiques.",
    ),
    (
        "is_alphanumeric",
        "value.is_alphanumeric() -> bool",
        "Teste les caractères alphanumériques.",
    ),
    ("reverse", "value.reverse() -> str", "Inverse la chaîne."),
    ("iter", "value.iter()", "Renvoie un itérateur."),
];

/// Méthodes réelles de `Dict<K, V>`.
pub const DICT_METHODS: &[(&str, &str, &str)] = &[
    ("size", "value.size() -> int", "Nombre de paires."),
    (
        "is_empty",
        "value.is_empty() -> bool",
        "Indique si le dictionnaire est vide.",
    ),
    (
        "contains",
        "value.contains(key) -> bool",
        "Teste l'existence d'une clé.",
    ),
    (
        "entries",
        "value.entries()",
        "Renvoie les paires dans l'ordre d'insertion.",
    ),
    (
        "to_string",
        "value.to_string() -> str",
        "Convertit en chaîne.",
    ),
    ("get", "value.get(key) -> V", "Lit une valeur."),
    (
        "set",
        "value.set(key, value)",
        "Associe une valeur à une clé.",
    ),
    ("remove", "value.remove(key) -> V", "Retire une clé."),
    ("keys", "value.keys() -> List<K>", "Renvoie les clés."),
    (
        "values",
        "value.values() -> List<V>",
        "Renvoie les valeurs.",
    ),
    ("clear", "value.clear()", "Vide le dictionnaire."),
    (
        "get_or",
        "value.get_or(key, default) -> V",
        "Lit une clé avec valeur par défaut.",
    ),
    (
        "update",
        "value.update(other)",
        "Fusionne un autre dictionnaire.",
    ),
    ("copy", "value.copy() -> Dict<K, V>", "Copie superficielle."),
    ("iter", "value.iter()", "Renvoie un itérateur."),
];

/// Méthodes réelles de `Set<T>`.
pub const SET_METHODS: &[(&str, &str, &str)] = &[
    ("add", "value.add(element) -> bool", "Ajoute un élément."),
    (
        "remove",
        "value.remove(element) -> bool",
        "Supprime un élément.",
    ),
    (
        "contains",
        "value.contains(element) -> bool",
        "Teste l'appartenance.",
    ),
    ("size", "value.size() -> int", "Nombre d'éléments."),
    (
        "is_empty",
        "value.is_empty() -> bool",
        "Indique si l'ensemble est vide.",
    ),
    ("clear", "value.clear()", "Vide l'ensemble."),
    ("copy", "value.copy() -> Set<T>", "Copie superficielle."),
    (
        "to_list",
        "value.to_list() -> List<T>",
        "Convertit en List.",
    ),
    (
        "union",
        "value.union(other) -> Set<T>",
        "Union de deux ensembles.",
    ),
    (
        "intersection",
        "value.intersection(other) -> Set<T>",
        "Intersection.",
    ),
    (
        "difference",
        "value.difference(other) -> Set<T>",
        "Différence.",
    ),
    (
        "symmetric_difference",
        "value.symmetric_difference(other) -> Set<T>",
        "Différence symétrique.",
    ),
    (
        "is_subset",
        "value.is_subset(other) -> bool",
        "Teste si l'ensemble est inclus.",
    ),
    (
        "is_superset",
        "value.is_superset(other) -> bool",
        "Teste si l'ensemble contient l'autre.",
    ),
    ("equals", "value.equals(other) -> bool", "Teste l'égalité."),
    (
        "to_string",
        "value.to_string() -> str",
        "Convertit en chaîne.",
    ),
    ("iter", "value.iter()", "Renvoie un itérateur."),
];

/// Méthodes des tuples immuables.
pub const TUPLE_METHODS: &[(&str, &str, &str)] = &[
    ("size", "value.size() -> int", "Nombre d'éléments."),
    (
        "is_empty",
        "value.is_empty() -> bool",
        "Indique si le tuple est vide.",
    ),
    (
        "to_string",
        "value.to_string() -> str",
        "Convertit en chaîne.",
    ),
    ("get", "value.get(index) -> T", "Lit un élément."),
    (
        "contains",
        "value.contains(value) -> bool",
        "Teste l'appartenance.",
    ),
    ("index_of", "value.index_of(value) -> int", "Premier index."),
    ("first", "value.first() -> T", "Premier élément."),
    ("last", "value.last() -> T", "Dernier élément."),
    (
        "to_list",
        "value.to_list() -> List<T>",
        "Convertit en List.",
    ),
    ("iter", "value.iter()", "Renvoie un itérateur."),
];

/// Méthodes de `Range`.
pub const RANGE_METHODS: &[(&str, &str, &str)] = &[
    ("size", "value.size() -> int", "Nombre d'éléments produits."),
    (
        "is_empty",
        "value.is_empty() -> bool",
        "Indique si la range est vide.",
    ),
    ("start", "value.start() -> int", "Borne de départ."),
    ("stop", "value.stop() -> int", "Borne de fin exclusive."),
    ("step", "value.step() -> int", "Pas."),
    (
        "to_string",
        "value.to_string() -> str",
        "Convertit en chaîne.",
    ),
    ("iter", "value.iter()", "Renvoie un itérateur."),
];
