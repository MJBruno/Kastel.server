//! Métadonnées de langage du LSP Kastel.
//!
//! Cette table est volontairement alignée sur le lexer, le système de types
//! et les dispatchers réels de `kastel`.
//!
//! Les tables de méthodes des types de base (`List`, `Dict`, `Set`, `Tuple`,
//! `str`, `Range`, `Option`, `Result`, `Task`, `Channel`, `Mutex`,
//! `Semaphore`, `WaitGroup`, `Iterator`) doivent rester alignées sur
//! `compiler/types.rs` (`*_member_type`) : les tests en bas de ce fichier
//! échouent dès qu'une méthode est déclarée d'un côté seulement.

use kastel::compiler::types::Type;

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
    "self",
    "interface",
    "enum",
    "try",
    "catch",
    "throw",
    "finally",
    "is",
];

/// Types disponibles dans la syntaxe Kastel.
pub const TYPE_NAMES: &[&str] = &[
    "int",
    "float",
    "str",
    "bool",
    "None",
    "List",
    "Dict",
    "Tuple",
    "Set",
    "Range",
    "dynamic",
    "any",
    // Types génériques / handles fournis par le runtime.
    "Option",
    "Result",
    "Task",
    "Channel",
    "Mutex",
    "Semaphore",
    "WaitGroup",
    "Iterator",
    "Iterable",
    // Type du contrat implémenté, valide dans une `interface`.
    "Self",
];

/// Mots contextuels utilisés par les membres de classe et les alias.
///
/// `public`/`protected`/`private`/`static` ne sont reconnus par le parser
/// que dans le corps d'une classe (voir `frontend/parser/classes.rs`) ;
/// `type` n'est un mot-clé que dans `type Nom = ...;` (voir
/// `frontend/parser/imports.rs` et `statements.rs`).
pub const CONTEXTUAL_KEYWORDS: &[&str] = &["public", "protected", "private", "static", "type"];

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
    (
        "process_run",
        "process_run(program, args) -> Dict",
        "Lance un programme sans passer par un shell et renvoie son résultat.",
    ),
    // Option / Result.
    (
        "Some",
        "Some(value) -> Option<T>",
        "Construit une Option contenant une valeur.",
    ),
    (
        "Ok",
        "Ok(value) -> Result<T, E>",
        "Construit un Result réussi.",
    ),
    (
        "Err",
        "Err(error) -> Result<T, E>",
        "Construit un Result en erreur.",
    ),
    // Concurrence : intrinsèques compilés en opcodes ou natives du runtime.
    (
        "spawn",
        "spawn(function, ...args) -> Task<T>",
        "Démarre une tâche coopérative ; `join()` récupère son résultat.",
    ),
    (
        "yield",
        "yield()",
        "Rend la main au scheduler.",
    ),
    (
        "sleep",
        "sleep(milliseconds)",
        "Suspend la tâche courante pendant la durée donnée.",
    ),
    (
        "select",
        "select(cases, timeout_ms?)",
        "Attend le premier channel prêt (réception, ou envoi via `(channel, valeur)`) ; renvoie un tuple décrivant le cas retenu.",
    ),
    (
        "channel",
        "channel(capacity?) -> Channel<T>",
        "Crée un channel (non borné, ou borné si une capacité > 0 est donnée).",
    ),
    (
        "mutex",
        "mutex() -> Mutex",
        "Crée un verrou d'exclusion mutuelle.",
    ),
    (
        "semaphore",
        "semaphore(permits) -> Semaphore",
        "Crée un sémaphore avec le nombre de permis donné.",
    ),
    (
        "wait_group",
        "wait_group() -> WaitGroup",
        "Crée un WaitGroup pour attendre un groupe de tâches.",
    ),
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
    "process_run",
    "Some",
    "Ok",
    "Err",
    "spawn",
    "yield",
    "sleep",
    "select",
    "channel",
    "mutex",
    "semaphore",
    "wait_group",
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
    (
        "map",
        "value.map(function) -> List<U>",
        "Applique la fonction à chaque élément.",
    ),
    (
        "filter",
        "value.filter(predicate) -> List<T>",
        "Garde les éléments qui satisfont le prédicat.",
    ),
    (
        "reduce",
        "value.reduce(initial, function) -> U",
        "Replie la liste sur une valeur initiale.",
    ),
    (
        "any",
        "value.any(predicate) -> bool",
        "Vrai si au moins un élément satisfait le prédicat.",
    ),
    (
        "all",
        "value.all(predicate) -> bool",
        "Vrai si tous les éléments satisfont le prédicat.",
    ),
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

/// Méthodes de `Option<T>` (voir `Type::option_result_member_type`).
pub const OPTION_METHODS: &[(&str, &str, &str)] = &[
    (
        "is_some",
        "value.is_some() -> bool",
        "Vrai si l'Option contient une valeur.",
    ),
    (
        "is_none",
        "value.is_none() -> bool",
        "Vrai si l'Option est vide.",
    ),
    (
        "unwrap",
        "value.unwrap() -> T",
        "Extrait la valeur ; échoue si l'Option est vide.",
    ),
    (
        "expect",
        "value.expect(message) -> T",
        "Extrait la valeur ; échoue avec le message si l'Option est vide.",
    ),
    (
        "unwrap_or",
        "value.unwrap_or(default) -> T",
        "Extrait la valeur, ou renvoie la valeur par défaut.",
    ),
    (
        "map",
        "value.map(function) -> Option<U>",
        "Transforme la valeur contenue.",
    ),
    (
        "and_then",
        "value.and_then(function) -> Option<U>",
        "Enchaîne une fonction qui renvoie une Option.",
    ),
    (
        "ok_or",
        "value.ok_or(error) -> Result<T, E>",
        "Convertit l'Option en Result.",
    ),
    (
        "to_string",
        "value.to_string() -> str",
        "Convertit en chaîne.",
    ),
];

/// Méthodes de `Result<T, E>` (voir `Type::option_result_member_type`).
pub const RESULT_METHODS: &[(&str, &str, &str)] = &[
    (
        "is_ok",
        "value.is_ok() -> bool",
        "Vrai si le Result est réussi.",
    ),
    (
        "is_err",
        "value.is_err() -> bool",
        "Vrai si le Result est en erreur.",
    ),
    (
        "unwrap",
        "value.unwrap() -> T",
        "Extrait la valeur ; échoue si le Result est en erreur.",
    ),
    (
        "expect",
        "value.expect(message) -> T",
        "Extrait la valeur ; échoue avec le message en cas d'erreur.",
    ),
    (
        "unwrap_err",
        "value.unwrap_err() -> E",
        "Extrait l'erreur ; échoue si le Result est réussi.",
    ),
    (
        "expect_err",
        "value.expect_err(message) -> E",
        "Extrait l'erreur ; échoue avec le message si le Result est réussi.",
    ),
    (
        "unwrap_or",
        "value.unwrap_or(default) -> T",
        "Extrait la valeur, ou renvoie la valeur par défaut.",
    ),
    (
        "map",
        "value.map(function) -> Result<U, E>",
        "Transforme la valeur de succès.",
    ),
    (
        "map_err",
        "value.map_err(function) -> Result<T, F>",
        "Transforme l'erreur.",
    ),
    (
        "and_then",
        "value.and_then(function) -> Result<U, E>",
        "Enchaîne une fonction qui renvoie un Result.",
    ),
    (
        "ok",
        "value.ok() -> Option<T>",
        "Convertit en Option de la valeur de succès.",
    ),
    (
        "err",
        "value.err() -> Option<E>",
        "Convertit en Option de l'erreur.",
    ),
    (
        "to_string",
        "value.to_string() -> str",
        "Convertit en chaîne.",
    ),
];

/// Méthodes de `Task<T>` (résultat de `spawn`).
pub const TASK_METHODS: &[(&str, &str, &str)] = &[
    (
        "join",
        "value.join() -> T",
        "Attend la fin de la tâche et renvoie son résultat.",
    ),
    (
        "status",
        "value.status() -> str",
        "État courant de la tâche.",
    ),
    (
        "is_done",
        "value.is_done() -> bool",
        "Vrai si la tâche est terminée.",
    ),
    ("cancel", "value.cancel()", "Demande l'annulation de la tâche."),
];

/// Méthodes de `Channel<T>`.
pub const CHANNEL_METHODS: &[(&str, &str, &str)] = &[
    (
        "send",
        "value.send(element)",
        "Envoie un élément (suspend si le channel borné est plein).",
    ),
    (
        "try_send",
        "value.try_send(element) -> bool",
        "Envoie sans bloquer ; renvoie false si le channel est plein.",
    ),
    (
        "recv",
        "value.recv() -> T",
        "Reçoit un élément (suspend tant que le channel est vide).",
    ),
    (
        "try_recv",
        "value.try_recv() -> Option<T>",
        "Reçoit sans bloquer ; renvoie None si le channel est vide.",
    ),
    (
        "size",
        "value.size() -> int",
        "Nombre d'éléments en attente.",
    ),
    (
        "capacity",
        "value.capacity() -> Option<int>",
        "Capacité du channel, ou None s'il n'est pas borné.",
    ),
    (
        "is_full",
        "value.is_full() -> bool",
        "Vrai si le channel borné est plein.",
    ),
    (
        "is_empty",
        "value.is_empty() -> bool",
        "Vrai si aucun élément n'est en attente.",
    ),
    ("close", "value.close()", "Ferme le channel."),
    (
        "is_closed",
        "value.is_closed() -> bool",
        "Vrai si le channel est fermé.",
    ),
];

/// Méthodes de `Mutex`.
pub const MUTEX_METHODS: &[(&str, &str, &str)] = &[
    ("lock", "value.lock()", "Prend le verrou (suspend s'il est pris)."),
    ("unlock", "value.unlock()", "Libère le verrou."),
    (
        "try_lock",
        "value.try_lock() -> bool",
        "Tente de prendre le verrou sans bloquer.",
    ),
    (
        "is_locked",
        "value.is_locked() -> bool",
        "Vrai si le verrou est pris.",
    ),
];

/// Méthodes de `Semaphore`.
pub const SEMAPHORE_METHODS: &[(&str, &str, &str)] = &[
    (
        "acquire",
        "value.acquire()",
        "Prend un permis (suspend s'il n'y en a plus).",
    ),
    ("release", "value.release()", "Rend un permis."),
    (
        "try_acquire",
        "value.try_acquire() -> bool",
        "Tente de prendre un permis sans bloquer.",
    ),
    (
        "available",
        "value.available() -> int",
        "Nombre de permis disponibles.",
    ),
    (
        "capacity",
        "value.capacity() -> int",
        "Nombre total de permis.",
    ),
];

/// Méthodes de `WaitGroup`.
pub const WAIT_GROUP_METHODS: &[(&str, &str, &str)] = &[
    (
        "add",
        "value.add(count)",
        "Ajoute des tâches à attendre.",
    ),
    ("done", "value.done()", "Signale la fin d'une tâche."),
    (
        "wait",
        "value.wait()",
        "Attend que le compteur retombe à zéro.",
    ),
    (
        "count",
        "value.count() -> int",
        "Nombre de tâches encore attendues.",
    ),
    (
        "is_done",
        "value.is_done() -> bool",
        "Vrai si plus aucune tâche n'est attendue.",
    ),
];

/// Méthodes d'un itérateur (`iter()`, ou classe implémentant `Iterator<T>`).
pub const ITERATOR_METHODS: &[(&str, &str, &str)] = &[
    (
        "next",
        "value.next() -> T",
        "Renvoie l'élément suivant.",
    ),
    (
        "has_next",
        "value.has_next() -> bool",
        "Vrai s'il reste un élément.",
    ),
    (
        "peek",
        "value.peek() -> T",
        "Lit l'élément suivant sans l'avancer.",
    ),
    (
        "map",
        "value.map(function) -> Iterator<U>",
        "Transforme paresseusement chaque élément.",
    ),
    (
        "filter",
        "value.filter(predicate) -> Iterator<T>",
        "Filtre paresseusement les éléments.",
    ),
    (
        "take",
        "value.take(count) -> Iterator<T>",
        "Limite le nombre d'éléments.",
    ),
    (
        "skip",
        "value.skip(count) -> Iterator<T>",
        "Ignore les premiers éléments.",
    ),
    (
        "collect",
        "value.collect() -> List<T>",
        "Consomme l'itérateur dans une List.",
    ),
    (
        "to_list",
        "value.to_list() -> List<T>",
        "Consomme l'itérateur dans une List.",
    ),
    (
        "count",
        "value.count() -> int",
        "Compte les éléments restants.",
    ),
    (
        "any",
        "value.any(predicate) -> bool",
        "Vrai si un élément satisfait le prédicat.",
    ),
    (
        "all",
        "value.all(predicate) -> bool",
        "Vrai si tous les éléments satisfont le prédicat.",
    ),
    ("iter", "value.iter()", "Renvoie l'itérateur lui-même."),
];

/// Méthodes communes aux `Record` (les champs sont complétés à part).
pub const RECORD_METHODS: &[(&str, &str, &str)] = &[
    ("keys", "value.keys() -> List<str>", "Noms des champs."),
    (
        "values",
        "value.values() -> List<T>",
        "Valeurs des champs.",
    ),
    (
        "entries",
        "value.entries()",
        "Paires (nom, valeur) des champs.",
    ),
    ("copy", "value.copy()", "Copie superficielle."),
    (
        "to_string",
        "value.to_string() -> str",
        "Convertit en chaîne.",
    ),
];

/// Table de méthodes intégrées associée à un type statique, s'il en existe une.
///
/// Un `Type::Named` correspondant à un handle du runtime (`Mutex`,
/// `Semaphore`, `WaitGroup`) n'est reconnu qu'ici : l'appelant doit d'abord
/// vérifier qu'une classe utilisateur de même nom n'existe pas.
pub fn member_table(ty: &Type) -> Option<&'static [(&'static str, &'static str, &'static str)]> {
    match ty {
        Type::Array(_) | Type::ArrayDynamic => Some(LIST_METHODS),
        Type::Dict(_, _) | Type::DictDynamic => Some(DICT_METHODS),
        Type::Tuple(_) | Type::TupleDynamic => Some(TUPLE_METHODS),
        Type::Set(_) | Type::SetDynamic => Some(SET_METHODS),
        Type::Str => Some(STRING_METHODS),
        Type::Range => Some(RANGE_METHODS),
        Type::Record(_) => Some(RECORD_METHODS),
        Type::Generic { name, .. } => match name.to_ascii_lowercase().as_str() {
            "option" => Some(OPTION_METHODS),
            "result" => Some(RESULT_METHODS),
            "task" => Some(TASK_METHODS),
            "channel" => Some(CHANNEL_METHODS),
            "iterator" => Some(ITERATOR_METHODS),
            _ => None,
        },
        Type::Named(name) => builtin_handle_table(name),
        _ => None,
    }
}

/// Table des handles du runtime exposés comme `Type::Named`.
pub fn builtin_handle_table(name: &str) -> Option<&'static [(&'static str, &'static str, &'static str)]> {
    match name.to_ascii_lowercase().as_str() {
        "mutex" => Some(MUTEX_METHODS),
        "semaphore" => Some(SEMAPHORE_METHODS),
        "waitgroup" => Some(WAIT_GROUP_METHODS),
        "iterator" => Some(ITERATOR_METHODS),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Table = &'static [(&'static str, &'static str, &'static str)];

    fn option_of(element: Type) -> Type {
        Type::Generic {
            name: "Option".into(),
            arguments: vec![element],
        }
    }

    fn result_of(ok: Type, err: Type) -> Type {
        Type::Generic {
            name: "Result".into(),
            arguments: vec![ok, err],
        }
    }

    fn generic_of(name: &str, element: Type) -> Type {
        Type::Generic {
            name: name.into(),
            arguments: vec![element],
        }
    }

    fn universe() -> Vec<&'static str> {
        let tables: [Table; 15] = [
            LIST_METHODS,
            STRING_METHODS,
            DICT_METHODS,
            SET_METHODS,
            TUPLE_METHODS,
            RANGE_METHODS,
            OPTION_METHODS,
            RESULT_METHODS,
            TASK_METHODS,
            CHANNEL_METHODS,
            MUTEX_METHODS,
            SEMAPHORE_METHODS,
            WAIT_GROUP_METHODS,
            ITERATOR_METHODS,
            RECORD_METHODS,
        ];
        let mut names: Vec<&'static str> = tables
            .iter()
            .flat_map(|table| table.iter().map(|(name, _, _)| *name))
            .collect();
        // Noms supprimés de l'API standard : ils ne doivent jamais apparaître.
        names.extend(["length", "push", "has", "items", "to_iterator", "to_array"]);
        names.sort_unstable();
        names.dedup();
        names
    }

    /// Chaque méthode listée doit être connue du vérificateur de types, pour
    /// les types dont `types.rs` expose une table complète.
    #[test]
    fn tables_listed_methods_are_known_to_the_type_checker() {
        let set = Type::Set(Box::new(Type::Int));
        for (name, _, _) in SET_METHODS {
            assert!(set.set_member_type(name).is_some(), "Set.{name}");
        }

        let option = option_of(Type::Int);
        for (name, _, _) in OPTION_METHODS {
            assert!(
                option.option_result_member_type(name).is_some(),
                "Option.{name}"
            );
        }

        let result = result_of(Type::Int, Type::Str);
        for (name, _, _) in RESULT_METHODS {
            assert!(
                result.option_result_member_type(name).is_some(),
                "Result.{name}"
            );
        }

        let task = generic_of("Task", Type::Int);
        for (name, _, _) in TASK_METHODS {
            assert!(task.task_member_type(name).is_some(), "Task.{name}");
        }

        let channel = generic_of("Channel", Type::Int);
        for (name, _, _) in CHANNEL_METHODS {
            assert!(channel.channel_member_type(name).is_some(), "Channel.{name}");
        }

        let mutex = Type::Named("Mutex".into());
        for (name, _, _) in MUTEX_METHODS {
            assert!(mutex.mutex_member_type(name).is_some(), "Mutex.{name}");
        }

        let semaphore = Type::Named("Semaphore".into());
        for (name, _, _) in SEMAPHORE_METHODS {
            assert!(
                semaphore.semaphore_member_type(name).is_some(),
                "Semaphore.{name}"
            );
        }

        let wait_group = Type::Named("WaitGroup".into());
        for (name, _, _) in WAIT_GROUP_METHODS {
            assert!(
                wait_group.wait_group_member_type(name).is_some(),
                "WaitGroup.{name}"
            );
        }

        let record = Type::Record(vec![("a".into(), Type::Int)]);
        for (name, _, _) in RECORD_METHODS {
            assert!(record.record_method_type(name).is_some(), "Record.{name}");
        }
    }

    /// Réciproque : une méthode connue du vérificateur de types doit figurer
    /// dans la table du LSP correspondante (sinon complétion/hover incomplets).
    #[test]
    fn type_checker_methods_are_listed_in_tables() {
        let names = universe();

        let cases: Vec<(Type, Table)> = vec![
            (Type::Array(Box::new(Type::Int)), LIST_METHODS),
            (Type::Dict(Box::new(Type::Str), Box::new(Type::Int)), DICT_METHODS),
            (Type::Tuple(vec![Type::Int]), TUPLE_METHODS),
            (Type::Str, STRING_METHODS),
            (Type::Range, RANGE_METHODS),
        ];
        for (ty, table) in cases {
            for name in &names {
                if ty.collection_member_type(name).is_some() {
                    assert!(
                        table.iter().any(|(n, _, _)| n == name),
                        "{ty}: `{name}` connu du type checker mais absent de la table LSP"
                    );
                }
            }
        }

        let set = Type::Set(Box::new(Type::Int));
        let option = option_of(Type::Int);
        let result = result_of(Type::Int, Type::Str);
        let task = generic_of("Task", Type::Int);
        let channel = generic_of("Channel", Type::Int);
        let mutex = Type::Named("Mutex".into());
        let semaphore = Type::Named("Semaphore".into());
        let wait_group = Type::Named("WaitGroup".into());
        let record = Type::Record(vec![("a".into(), Type::Int)]);

        for name in &names {
            let listed = |table: Table| table.iter().any(|(n, _, _)| n == name);
            if set.set_member_type(name).is_some() {
                assert!(listed(SET_METHODS), "Set.{name}");
            }
            if option.option_result_member_type(name).is_some() {
                assert!(listed(OPTION_METHODS), "Option.{name}");
            }
            if result.option_result_member_type(name).is_some() {
                assert!(listed(RESULT_METHODS), "Result.{name}");
            }
            if task.task_member_type(name).is_some() {
                assert!(listed(TASK_METHODS), "Task.{name}");
            }
            if channel.channel_member_type(name).is_some() {
                assert!(listed(CHANNEL_METHODS), "Channel.{name}");
            }
            if mutex.mutex_member_type(name).is_some() {
                assert!(listed(MUTEX_METHODS), "Mutex.{name}");
            }
            if semaphore.semaphore_member_type(name).is_some() {
                assert!(listed(SEMAPHORE_METHODS), "Semaphore.{name}");
            }
            if wait_group.wait_group_member_type(name).is_some() {
                assert!(listed(WAIT_GROUP_METHODS), "WaitGroup.{name}");
            }
            if record.record_method_type(name).is_some() {
                assert!(listed(RECORD_METHODS), "Record.{name}");
            }
        }
    }

    #[test]
    fn removed_collection_methods_are_not_advertised() {
        for removed in ["length", "push", "has", "items", "to_iterator", "to_array"] {
            for table in [
                LIST_METHODS,
                DICT_METHODS,
                SET_METHODS,
                TUPLE_METHODS,
                STRING_METHODS,
                RANGE_METHODS,
            ] {
                assert!(
                    !table.iter().any(|(name, _, _)| *name == removed),
                    "`{removed}` a été supprimé de l'API standard"
                );
            }
        }
    }

    #[test]
    fn member_table_resolves_runtime_types() {
        assert!(member_table(&option_of(Type::Int)).is_some());
        assert!(member_table(&result_of(Type::Int, Type::Str)).is_some());
        assert!(member_table(&generic_of("Task", Type::Int)).is_some());
        assert!(member_table(&generic_of("Channel", Type::Int)).is_some());
        assert!(member_table(&Type::Named("Mutex".into())).is_some());
        assert!(member_table(&Type::Named("WaitGroup".into())).is_some());
        assert!(member_table(&Type::Named("MaClasse".into())).is_none());
        assert!(member_table(&Type::Dynamic).is_none());
    }

    #[test]
    fn concurrency_and_result_builtins_are_registered() {
        for name in [
            "spawn",
            "yield",
            "sleep",
            "select",
            "channel",
            "mutex",
            "semaphore",
            "wait_group",
            "Some",
            "Ok",
            "Err",
            "process_run",
        ] {
            assert!(BUILTINS.contains(&name), "{name} absent de BUILTINS");
            assert!(
                BUILTIN_FUNCTIONS.iter().any(|(n, _, _)| *n == name),
                "{name} absent de BUILTIN_FUNCTIONS"
            );
        }
    }

    #[test]
    fn builtin_tables_are_consistent() {
        for (name, _, _) in BUILTIN_FUNCTIONS {
            assert!(BUILTINS.contains(name), "{name} absent de BUILTINS");
        }
        for name in BUILTINS {
            assert!(
                BUILTIN_FUNCTIONS.iter().any(|(n, _, _)| n == name),
                "{name} absent de BUILTIN_FUNCTIONS"
            );
        }
    }
}
