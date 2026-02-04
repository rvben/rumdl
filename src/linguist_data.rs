//! GitHub Linguist language data
//!
//! Auto-generated from GitHub Linguist languages.yml
//! Linguist commit: e51c2270 (2026-01-14)
//!
//! To regenerate: `uv run --with pyyaml python /tmp/gen_linguist.py > src/linguist_data.rs`

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

/// Linguist commit hash this data was generated from
pub const LINGUIST_VERSION: &str = "e51c2270";

/// Linguist commit date
pub const LINGUIST_DATE: &str = "2026-01-14";

/// Maps lowercase alias -> canonical language name
pub static ALIAS_TO_CANONICAL: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::with_capacity(1208);
    m.insert("1c enterprise", "1C Enterprise");
    m.insert("2-dimensional array", "2-Dimensional Array");
    m.insert("4d", "4D");
    m.insert("abap", "ABAP");
    m.insert("abap cds", "ABAP CDS");
    m.insert("abl", "OpenEdge ABL");
    m.insert("abnf", "ABNF");
    m.insert("abuild", "Shell");
    m.insert("acfm", "Adobe Font Metrics");
    m.insert("ackrc", "Option List");
    m.insert("aconf", "ApacheConf");
    m.insert("actionscript", "ActionScript");
    m.insert("actionscript 3", "ActionScript");
    m.insert("actionscript3", "ActionScript");
    m.insert("ad block", "Adblock Filter List");
    m.insert("ad block filters", "Adblock Filter List");
    m.insert("ada", "Ada");
    m.insert("ada2005", "Ada");
    m.insert("ada95", "Ada");
    m.insert("adb", "Adblock Filter List");
    m.insert("adblock", "Adblock Filter List");
    m.insert("adblock filter list", "Adblock Filter List");
    m.insert("adobe composite font metrics", "Adobe Font Metrics");
    m.insert("adobe font metrics", "Adobe Font Metrics");
    m.insert("adobe multiple font metrics", "Adobe Font Metrics");
    m.insert("advpl", "xBase");
    m.insert("afdko", "OpenType Feature File");
    m.insert("agda", "Agda");
    m.insert("ags", "AGS Script");
    m.insert("ags script", "AGS Script");
    m.insert("ahk", "AutoHotkey");
    m.insert("aidl", "AIDL");
    m.insert("aiken", "Aiken");
    m.insert("al", "AL");
    m.insert("algol", "ALGOL");
    m.insert("alloy", "Alloy");
    m.insert("alpine abuild", "Shell");
    m.insert("altium", "Altium Designer");
    m.insert("altium designer", "Altium Designer");
    m.insert("amfm", "Adobe Font Metrics");
    m.insert("ampl", "AMPL");
    m.insert("amusewiki", "Muse");
    m.insert("angelscript", "AngelScript");
    m.insert("answer set programming", "Answer Set Programming");
    m.insert("ant build system", "Ant Build System");
    m.insert("antlers", "Antlers");
    m.insert("antlr", "ANTLR");
    m.insert("apache", "ApacheConf");
    m.insert("apacheconf", "ApacheConf");
    m.insert("apex", "Apex");
    m.insert("api blueprint", "API Blueprint");
    m.insert("apkbuild", "Shell");
    m.insert("apl", "APL");
    m.insert("apollo guidance computer", "Assembly");
    m.insert("applescript", "AppleScript");
    m.insert("arc", "Arc");
    m.insert("arexx", "REXX");
    m.insert("as3", "ActionScript");
    m.insert("ascii stl", "STL");
    m.insert("asciidoc", "AsciiDoc");
    m.insert("asl", "ASL");
    m.insert("asm", "Assembly");
    m.insert("asn.1", "ASN.1");
    m.insert("asp", "Classic ASP");
    m.insert("asp.net", "ASP.NET");
    m.insert("aspectj", "AspectJ");
    m.insert("aspx", "ASP.NET");
    m.insert("aspx-vb", "ASP.NET");
    m.insert("assembly", "Assembly");
    m.insert("astro", "Astro");
    m.insert("asymptote", "Asymptote");
    m.insert("ats", "ATS");
    m.insert("ats2", "ATS");
    m.insert("au3", "AutoIt");
    m.insert("augeas", "Augeas");
    m.insert("autoconf", "M4");
    m.insert("autohotkey", "AutoHotkey");
    m.insert("autoit", "AutoIt");
    m.insert("autoit3", "AutoIt");
    m.insert("autoitscript", "AutoIt");
    m.insert("avro idl", "Avro IDL");
    m.insert("awk", "Awk");
    m.insert("b (formal method)", "B (Formal Method)");
    m.insert("b3d", "BlitzBasic");
    m.insert("b4x", "B4X");
    m.insert("ballerina", "Ballerina");
    m.insert("bash", "Shell");
    m.insert("bash session", "ShellSession");
    m.insert("basic", "BASIC");
    m.insert("basic for android", "B4X");
    m.insert("bat", "Batchfile");
    m.insert("batch", "Batchfile");
    m.insert("batchfile", "Batchfile");
    m.insert("bazel", "Starlark");
    m.insert("be", "Berry");
    m.insert("beef", "Beef");
    m.insert("befunge", "Befunge");
    m.insert("berry", "Berry");
    m.insert("bh", "Bluespec");
    m.insert("bibtex", "TeX");
    m.insert("bibtex style", "BibTeX Style");
    m.insert("bicep", "Bicep");
    m.insert("bikeshed", "Bikeshed");
    m.insert("bison", "Yacc");
    m.insert("bitbake", "BitBake");
    m.insert("blade", "Blade");
    m.insert("blitz3d", "BlitzBasic");
    m.insert("blitzbasic", "BlitzBasic");
    m.insert("blitzmax", "BlitzMax");
    m.insert("blitzplus", "BlitzBasic");
    m.insert("bluespec", "Bluespec");
    m.insert("bluespec bh", "Bluespec");
    m.insert("bluespec bsv", "Bluespec");
    m.insert("bluespec classic", "Bluespec");
    m.insert("bmax", "BlitzMax");
    m.insert("boo", "Boo");
    m.insert("boogie", "Boogie");
    m.insert("bplus", "BlitzBasic");
    m.insert("bqn", "BQN");
    m.insert("brainfuck", "Brainfuck");
    m.insert("brighterscript", "BrighterScript");
    m.insert("brightscript", "Brightscript");
    m.insert("bro", "Zeek");
    m.insert("browserslist", "Browserslist");
    m.insert("bru", "Bru");
    m.insert("bsdmake", "Makefile");
    m.insert("bsv", "Bluespec");
    m.insert("buildstream", "BuildStream");
    m.insert("byond", "DM");
    m.insert("bzl", "Starlark");
    m.insert("c", "C");
    m.insert("c#", "C#");
    m.insert("c++", "C++");
    m.insert("c++-objdump", "Cpp-ObjDump");
    m.insert("c-objdump", "C-ObjDump");
    m.insert("c2hs", "Haskell");
    m.insert("c2hs haskell", "Haskell");
    m.insert("c3", "C3");
    m.insert("cabal", "Cabal Config");
    m.insert("cabal config", "Cabal Config");
    m.insert("caddy", "Caddyfile");
    m.insert("caddyfile", "Caddyfile");
    m.insert("cadence", "Cadence");
    m.insert("cairo", "Cairo");
    m.insert("cairo zero", "Cairo");
    m.insert("cake", "C#");
    m.insert("cakescript", "C#");
    m.insert("cameligo", "LigoLANG");
    m.insert("cangjie", "Cangjie");
    m.insert("cap cds", "CAP CDS");
    m.insert("cap'n proto", "Cap'n Proto");
    m.insert("carbon", "Carbon");
    m.insert("carto", "CartoCSS");
    m.insert("cartocss", "CartoCSS");
    m.insert("cds", "CAP CDS");
    m.insert("ceylon", "Ceylon");
    m.insert("cfc", "ColdFusion");
    m.insert("cfm", "ColdFusion");
    m.insert("cfml", "ColdFusion");
    m.insert("chapel", "Chapel");
    m.insert("charity", "Charity");
    m.insert("checksum", "Checksums");
    m.insert("checksums", "Checksums");
    m.insert("chpl", "Chapel");
    m.insert("chuck", "ChucK");
    m.insert("cil", "CIL");
    m.insert("circom", "Circom");
    m.insert("cirru", "Cirru");
    m.insert("clarion", "Clarion");
    m.insert("clarity", "Clarity");
    m.insert("classic asp", "Classic ASP");
    m.insert("classic qbasic", "QuickBASIC");
    m.insert("classic quickbasic", "QuickBASIC");
    m.insert("classic visual basic", "Visual Basic 6.0");
    m.insert("clean", "Clean");
    m.insert("click", "Click");
    m.insert("clipper", "xBase");
    m.insert("clips", "CLIPS");
    m.insert("clojure", "Clojure");
    m.insert("closure templates", "Closure Templates");
    m.insert("cloud firestore security rules", "Cloud Firestore Security Rules");
    m.insert("clue", "Clue");
    m.insert("cmake", "CMake");
    m.insert("cobol", "COBOL");
    m.insert("coccinelle", "SmPL");
    m.insert("codeowners", "CODEOWNERS");
    m.insert("codeql", "CodeQL");
    m.insert("coffee", "CoffeeScript");
    m.insert("coffee-script", "CoffeeScript");
    m.insert("coffeescript", "CoffeeScript");
    m.insert("coldfusion", "ColdFusion");
    m.insert("coldfusion cfc", "ColdFusion");
    m.insert("coldfusion html", "ColdFusion");
    m.insert("collada", "COLLADA");
    m.insert("commit", "Git Commit");
    m.insert("common lisp", "Common Lisp");
    m.insert("common workflow language", "Common Workflow Language");
    m.insert("component pascal", "Component Pascal");
    m.insert("conll", "CoNLL-U");
    m.insert("conll-u", "CoNLL-U");
    m.insert("conll-x", "CoNLL-U");
    m.insert("console", "ShellSession");
    m.insert("containerfile", "Dockerfile");
    m.insert("cooklang", "Cooklang");
    m.insert("cool", "Cool");
    m.insert("coq", "Rocq Prover");
    m.insert("cperl", "Perl");
    m.insert("cpp", "C++");
    m.insert("cpp-objdump", "Cpp-ObjDump");
    m.insert("cql", "CQL");
    m.insert("creole", "Creole");
    m.insert("cron", "crontab");
    m.insert("cron table", "crontab");
    m.insert("crontab", "crontab");
    m.insert("crystal", "Crystal");
    m.insert("csharp", "C#");
    m.insert("cson", "CSON");
    m.insert("csound", "Csound");
    m.insert("csound document", "Csound Document");
    m.insert("csound score", "Csound Score");
    m.insert("csound-csd", "Csound Document");
    m.insert("csound-orc", "Csound");
    m.insert("csound-sco", "Csound Score");
    m.insert("css", "CSS");
    m.insert("csv", "CSV");
    m.insert("cucumber", "Gherkin");
    m.insert("cuda", "Cuda");
    m.insert("cue", "CUE");
    m.insert("cue sheet", "Cue Sheet");
    m.insert("curl config", "INI");
    m.insert("curlrc", "INI");
    m.insert("curry", "Curry");
    m.insert("cweb", "CWeb");
    m.insert("cwl", "Common Workflow Language");
    m.insert("cycript", "Cycript");
    m.insert("cylc", "INI");
    m.insert("cypher", "Cypher");
    m.insert("cython", "Cython");
    m.insert("d", "D");
    m.insert("d-objdump", "D-ObjDump");
    m.insert("d2", "D2");
    m.insert("d2lang", "D2");
    m.insert("dafny", "Dafny");
    m.insert("darcs patch", "Darcs Patch");
    m.insert("dart", "Dart");
    m.insert("daslang", "Daslang");
    m.insert("dataweave", "DataWeave");
    m.insert("dcl", "DIGITAL Command Language");
    m.insert("debian package control file", "Debian Package Control File");
    m.insert("delphi", "Pascal");
    m.insert("denizenscript", "DenizenScript");
    m.insert("desktop", "desktop");
    m.insert("dhall", "Dhall");
    m.insert("diff", "Diff");
    m.insert("digital command language", "DIGITAL Command Language");
    m.insert("dircolors", "dircolors");
    m.insert("directx 3d file", "DirectX 3D File");
    m.insert("django", "Jinja");
    m.insert("dlang", "D");
    m.insert("dm", "DM");
    m.insert("dns zone", "DNS Zone");
    m.insert("dockerfile", "Dockerfile");
    m.insert("dogescript", "Dogescript");
    m.insert("dosbatch", "Batchfile");
    m.insert("dosini", "INI");
    m.insert("dotenv", "Dotenv");
    m.insert("dpatch", "Darcs Patch");
    m.insert("dtrace", "DTrace");
    m.insert("dtrace-script", "DTrace");
    m.insert("dune", "Dune");
    m.insert("dylan", "Dylan");
    m.insert("e", "E");
    m.insert("e-mail", "E-mail");
    m.insert("eagle", "Eagle");
    m.insert("earthfile", "Earthly");
    m.insert("earthly", "Earthly");
    m.insert("easybuild", "Python");
    m.insert("ebnf", "EBNF");
    m.insert("ec", "eC");
    m.insert("ecere projects", "JavaScript");
    m.insert("ecl", "ECL");
    m.insert("eclipse", "Prolog");
    m.insert("ecmarkdown", "HTML");
    m.insert("ecmarkup", "HTML");
    m.insert("ecr", "HTML");
    m.insert("edge", "Edge");
    m.insert("edgeql", "EdgeQL");
    m.insert("editor-config", "INI");
    m.insert("editorconfig", "INI");
    m.insert("edje data collection", "Edje Data Collection");
    m.insert("edn", "edn");
    m.insert("eeschema schematic", "KiCad Schematic");
    m.insert("eex", "HTML");
    m.insert("eiffel", "Eiffel");
    m.insert("ejs", "EJS");
    m.insert("electronic business card", "vCard");
    m.insert("elisp", "Emacs Lisp");
    m.insert("elixir", "Elixir");
    m.insert("elm", "Elm");
    m.insert("elvish", "Elvish");
    m.insert("elvish transcript", "Elvish");
    m.insert("emacs", "Emacs Lisp");
    m.insert("emacs lisp", "Emacs Lisp");
    m.insert("emacs muse", "Muse");
    m.insert("email", "E-mail");
    m.insert("emberscript", "EmberScript");
    m.insert("eml", "E-mail");
    m.insert("envrc", "Shell");
    m.insert("eq", "EQ");
    m.insert("erb", "HTML");
    m.insert("erlang", "Erlang");
    m.insert("esdl", "EdgeQL");
    m.insert("euphoria", "Euphoria");
    m.insert("f#", "F#");
    m.insert("f*", "F*");
    m.insert("factor", "Factor");
    m.insert("fancy", "Fancy");
    m.insert("fantom", "Fantom");
    m.insert("faust", "Faust");
    m.insert("fb", "FreeBASIC");
    m.insert("fennel", "Fennel");
    m.insert("figfont", "FIGlet Font");
    m.insert("figlet font", "FIGlet Font");
    m.insert("filebench wml", "Filebench WML");
    m.insert("filterscript", "RenderScript");
    m.insert("firrtl", "FIRRTL");
    m.insert("fish", "Shell");
    m.insert("flex", "Lex");
    m.insert("flix", "Flix");
    m.insert("fluent", "Fluent");
    m.insert("flux", "FLUX");
    m.insert("formatted", "Formatted");
    m.insert("forth", "Forth");
    m.insert("fortran", "Fortran");
    m.insert("fortran free form", "Fortran");
    m.insert("foxpro", "xBase");
    m.insert("freebasic", "FreeBASIC");
    m.insert("freemarker", "FreeMarker");
    m.insert("frege", "Frege");
    m.insert("fsharp", "F#");
    m.insert("fstar", "F*");
    m.insert("ftl", "FreeMarker");
    m.insert("fundamental", "Text");
    m.insert("futhark", "Futhark");
    m.insert("g-code", "G-code");
    m.insert("game maker language", "Game Maker Language");
    m.insert("gaml", "GAML");
    m.insert("gams", "GAMS");
    m.insert("gap", "GAP");
    m.insert("gas", "Assembly");
    m.insert("gcc machine description", "GCC Machine Description");
    m.insert("gdb", "GDB");
    m.insert("gdscript", "GDScript");
    m.insert("gdshader", "GDShader");
    m.insert("gedcom", "GEDCOM");
    m.insert("gemfile.lock", "Gemfile.lock");
    m.insert("gemini", "Gemini");
    m.insert("gemtext", "Gemini");
    m.insert("genero 4gl", "Genero 4gl");
    m.insert("genero per", "Genero per");
    m.insert("genie", "Genie");
    m.insert("genshi", "Genshi");
    m.insert("gentoo ebuild", "Shell");
    m.insert("gentoo eclass", "Shell");
    m.insert("geojson", "JSON");
    m.insert("gerber image", "Gerber Image");
    m.insert("gettext catalog", "Gettext Catalog");
    m.insert("gf", "Grammatical Framework");
    m.insert("gherkin", "Gherkin");
    m.insert("git attributes", "Git Attributes");
    m.insert("git blame ignore revs", "Git Revision List");
    m.insert("git commit", "Git Commit");
    m.insert("git config", "INI");
    m.insert("git revision list", "Git Revision List");
    m.insert("git-ignore", "Ignore List");
    m.insert("gitattributes", "Git Attributes");
    m.insert("gitconfig", "INI");
    m.insert("gitignore", "Ignore List");
    m.insert("gitmodules", "INI");
    m.insert("gjs", "JavaScript");
    m.insert("gleam", "Gleam");
    m.insert("glimmer js", "JavaScript");
    m.insert("glimmer ts", "TypeScript");
    m.insert("glsl", "GLSL");
    m.insert("glyph", "Glyph");
    m.insert("glyph bitmap distribution format", "Glyph Bitmap Distribution Format");
    m.insert("gn", "GN");
    m.insert("gnu asm", "Assembly");
    m.insert("gnuplot", "Gnuplot");
    m.insert("go", "Go");
    m.insert("go checksums", "Go Checksums");
    m.insert("go mod", "Go Module");
    m.insert("go module", "Go Module");
    m.insert("go sum", "Go Checksums");
    m.insert("go template", "Go Template");
    m.insert("go work", "Go Workspace");
    m.insert("go work sum", "Go Checksums");
    m.insert("go workspace", "Go Workspace");
    m.insert("go.mod", "Go Module");
    m.insert("go.sum", "Go Checksums");
    m.insert("go.work", "Go Workspace");
    m.insert("go.work.sum", "Go Checksums");
    m.insert("godot resource", "Godot Resource");
    m.insert("golang", "Go");
    m.insert("golo", "Golo");
    m.insert("gosu", "Gosu");
    m.insert("gotmpl", "Go Template");
    m.insert("grace", "Grace");
    m.insert("gradle", "Gradle");
    m.insert("gradle kotlin dsl", "Gradle");
    m.insert("grammatical framework", "Grammatical Framework");
    m.insert("graph modeling language", "Graph Modeling Language");
    m.insert("graphql", "GraphQL");
    m.insert("graphviz (dot)", "Graphviz (DOT)");
    m.insert("groff", "Roff");
    m.insert("groovy", "Groovy");
    m.insert("groovy server pages", "Groovy");
    m.insert("gsc", "GSC");
    m.insert("gsp", "Groovy");
    m.insert("gts", "TypeScript");
    m.insert("hack", "Hack");
    m.insert("haml", "Haml");
    m.insert("handlebars", "Handlebars");
    m.insert("haproxy", "HAProxy");
    m.insert("harbour", "Harbour");
    m.insert("hare", "Hare");
    m.insert("hash", "Checksums");
    m.insert("hashes", "Checksums");
    m.insert("hashicorp configuration language", "HCL");
    m.insert("haskell", "Haskell");
    m.insert("haxe", "Haxe");
    m.insert("hbs", "Handlebars");
    m.insert("hcl", "HCL");
    m.insert("heex", "HTML");
    m.insert("help", "Vim Help File");
    m.insert("hip", "HIP");
    m.insert("hiveql", "HiveQL");
    m.insert("hls playlist", "M3U");
    m.insert("hlsl", "HLSL");
    m.insert("hocon", "HOCON");
    m.insert("holyc", "HolyC");
    m.insert("hoon", "hoon");
    m.insert("hosts", "Hosts File");
    m.insert("hosts file", "Hosts File");
    m.insert("html", "HTML");
    m.insert("html+django", "Jinja");
    m.insert("html+ecr", "HTML");
    m.insert("html+eex", "HTML");
    m.insert("html+erb", "HTML");
    m.insert("html+jinja", "Jinja");
    m.insert("html+php", "HTML");
    m.insert("html+razor", "HTML");
    m.insert("html+ruby", "HTML");
    m.insert("htmlbars", "Handlebars");
    m.insert("htmldjango", "Jinja");
    m.insert("http", "HTTP");
    m.insert("hurl", "Hurl");
    m.insert("hxml", "HXML");
    m.insert("hy", "Hy");
    m.insert("hylang", "Hy");
    m.insert("hyphy", "HyPhy");
    m.insert("i7", "Inform 7");
    m.insert("ical", "iCalendar");
    m.insert("icalendar", "iCalendar");
    m.insert("idl", "IDL");
    m.insert("idris", "Idris");
    m.insert("ignore", "Ignore List");
    m.insert("ignore list", "Ignore List");
    m.insert("igor", "IGOR Pro");
    m.insert("igor pro", "IGOR Pro");
    m.insert("igorpro", "IGOR Pro");
    m.insert("ijm", "ImageJ Macro");
    m.insert("ile rpg", "RPGLE");
    m.insert("imagej macro", "ImageJ Macro");
    m.insert("imba", "Imba");
    m.insert("inc", "PHP");
    m.insert("inform 7", "Inform 7");
    m.insert("inform7", "Inform 7");
    m.insert("ini", "INI");
    m.insert("ink", "Ink");
    m.insert("inno setup", "Inno Setup");
    m.insert("inputrc", "INI");
    m.insert("io", "Io");
    m.insert("ioke", "Ioke");
    m.insert("ipython notebook", "Jupyter Notebook");
    m.insert("irc", "IRC log");
    m.insert("irc log", "IRC log");
    m.insert("irc logs", "IRC log");
    m.insert("isabelle", "Isabelle");
    m.insert("isabelle root", "Isabelle");
    m.insert("ispc", "ISPC");
    m.insert("j", "J");
    m.insert("jac", "Jac");
    m.insert("jai", "Jai");
    m.insert("janet", "Janet");
    m.insert("jar manifest", "JAR Manifest");
    m.insert("jasmin", "Jasmin");
    m.insert("java", "Java");
    m.insert("java properties", "Java Properties");
    m.insert("java server page", "Groovy");
    m.insert("java server pages", "Java");
    m.insert("java template engine", "Java");
    m.insert("javascript", "JavaScript");
    m.insert("javascript+erb", "JavaScript");
    m.insert("jcl", "JCL");
    m.insert("jest snapshot", "Jest Snapshot");
    m.insert("jetbrains mps", "JetBrains MPS");
    m.insert("jflex", "Lex");
    m.insert("jinja", "Jinja");
    m.insert("jison", "Yacc");
    m.insert("jison lex", "Lex");
    m.insert("jolie", "Jolie");
    m.insert("jq", "jq");
    m.insert("jruby", "Ruby");
    m.insert("js", "JavaScript");
    m.insert("json", "JSON");
    m.insert("json with comments", "JSON");
    m.insert("json5", "JSON5");
    m.insert("jsonc", "JSON");
    m.insert("jsoniq", "JSONiq");
    m.insert("jsonl", "JSON");
    m.insert("jsonld", "JSONLD");
    m.insert("jsonnet", "Jsonnet");
    m.insert("jsp", "Java");
    m.insert("jte", "Java");
    m.insert("julia", "Julia");
    m.insert("julia repl", "Julia");
    m.insert("jupyter notebook", "Jupyter Notebook");
    m.insert("just", "Just");
    m.insert("justfile", "Just");
    m.insert("kaitai struct", "Kaitai Struct");
    m.insert("kak", "KakouneScript");
    m.insert("kakounescript", "KakouneScript");
    m.insert("kakscript", "KakouneScript");
    m.insert("kcl", "KCL");
    m.insert("kdl", "KDL");
    m.insert("kerboscript", "KerboScript");
    m.insert("keyvalues", "Valve Data Format");
    m.insert("kframework", "KFramework");
    m.insert("kicad layout", "KiCad Layout");
    m.insert("kicad legacy layout", "KiCad Legacy Layout");
    m.insert("kicad schematic", "KiCad Schematic");
    m.insert("kickstart", "Kickstart");
    m.insert("kit", "Kit");
    m.insert("koka", "Koka");
    m.insert("kolmafia ash", "KoLmafia ASH");
    m.insert("kotlin", "Kotlin");
    m.insert("krl", "KRL");
    m.insert("ksy", "Kaitai Struct");
    m.insert("kusto", "Kusto");
    m.insert("kvlang", "kvlang");
    m.insert("labview", "LabVIEW");
    m.insert("lambdapi", "Lambdapi");
    m.insert("langium", "Langium");
    m.insert("lark", "Lark");
    m.insert("lasso", "Lasso");
    m.insert("lassoscript", "Lasso");
    m.insert("latex", "TeX");
    m.insert("latte", "Latte");
    m.insert("lean", "Lean");
    m.insert("lean 4", "Lean");
    m.insert("lean4", "Lean");
    m.insert("leex", "HTML");
    m.insert("leo", "Leo");
    m.insert("less", "Less");
    m.insert("less-css", "Less");
    m.insert("lex", "Lex");
    m.insert("lfe", "LFE");
    m.insert("lhaskell", "Haskell");
    m.insert("lhs", "Haskell");
    m.insert("ligolang", "LigoLANG");
    m.insert("lilypond", "LilyPond");
    m.insert("limbo", "Limbo");
    m.insert("linear programming", "Linear Programming");
    m.insert("linker script", "Linker Script");
    m.insert("linux kernel module", "Linux Kernel Module");
    m.insert("liquid", "Liquid");
    m.insert("lisp", "Common Lisp");
    m.insert("litcoffee", "CoffeeScript");
    m.insert("literate agda", "Agda");
    m.insert("literate coffeescript", "CoffeeScript");
    m.insert("literate haskell", "Haskell");
    m.insert("live-script", "LiveScript");
    m.insert("livecode script", "LiveCode Script");
    m.insert("livescript", "LiveScript");
    m.insert("llvm", "LLVM");
    m.insert("logos", "Logos");
    m.insert("logtalk", "Logtalk");
    m.insert("lolcode", "LOLCODE");
    m.insert("lookml", "LookML");
    m.insert("loomscript", "LoomScript");
    m.insert("ls", "LiveScript");
    m.insert("lsl", "LSL");
    m.insert("ltspice symbol", "LTspice Symbol");
    m.insert("lua", "Lua");
    m.insert("luau", "Luau");
    m.insert("m", "M");
    m.insert("m2", "Macaulay2");
    m.insert("m3u", "M3U");
    m.insert("m3u playlist", "M3U");
    m.insert("m4", "M4");
    m.insert("m4sugar", "M4");
    m.insert("m68k", "Assembly");
    m.insert("macaulay2", "Macaulay2");
    m.insert("macruby", "Ruby");
    m.insert("mail", "E-mail");
    m.insert("make", "Makefile");
    m.insert("makefile", "Makefile");
    m.insert("mako", "Mako");
    m.insert("man", "Roff");
    m.insert("man page", "Roff");
    m.insert("man-page", "Roff");
    m.insert("manpage", "Roff");
    m.insert("markdown", "Markdown");
    m.insert("marko", "Marko");
    m.insert("markojs", "Marko");
    m.insert("mask", "Mask");
    m.insert("mathematica", "Wolfram Language");
    m.insert("mathematical programming system", "Mathematical Programming System");
    m.insert("matlab", "MATLAB");
    m.insert("maven pom", "XML");
    m.insert("max", "Max");
    m.insert("max/msp", "Max");
    m.insert("maxmsp", "Max");
    m.insert("maxscript", "MAXScript");
    m.insert("mbox", "E-mail");
    m.insert("mcfunction", "mcfunction");
    m.insert("md", "Markdown");
    m.insert("mdoc", "Roff");
    m.insert("mdsvex", "mdsvex");
    m.insert("mdx", "MDX");
    m.insert("mediawiki", "Wikitext");
    m.insert("mercury", "Mercury");
    m.insert("mermaid", "Mermaid");
    m.insert("mermaid example", "Mermaid");
    m.insert("meson", "Meson");
    m.insert("metal", "Metal");
    m.insert("mf", "Makefile");
    m.insert(
        "microsoft developer studio project",
        "Microsoft Developer Studio Project",
    );
    m.insert("microsoft visual studio solution", "Microsoft Visual Studio Solution");
    m.insert("minid", "MiniD");
    m.insert("miniyaml", "MiniYAML");
    m.insert("minizinc", "MiniZinc");
    m.insert("minizinc data", "MiniZinc Data");
    m.insert("mint", "Mint");
    m.insert("mirah", "Mirah");
    m.insert("mirc script", "mIRC Script");
    m.insert("mlir", "MLIR");
    m.insert("mma", "Wolfram Language");
    m.insert("modelica", "Modelica");
    m.insert("modula-2", "Modula-2");
    m.insert("modula-3", "Modula-3");
    m.insert("module management system", "Module Management System");
    m.insert("mojo", "Mojo");
    m.insert("monkey", "Monkey");
    m.insert("monkey c", "Monkey C");
    m.insert("moocode", "Moocode");
    m.insert("moonbit", "MoonBit");
    m.insert("moonscript", "MoonScript");
    m.insert("motoko", "Motoko");
    m.insert("motorola 68k assembly", "Assembly");
    m.insert("move", "Move");
    m.insert("mps", "JetBrains MPS");
    m.insert("mql4", "MQL4");
    m.insert("mql5", "MQL5");
    m.insert("mtml", "MTML");
    m.insert("muf", "Forth");
    m.insert("mumps", "M");
    m.insert("mupad", "mupad");
    m.insert("muse", "Muse");
    m.insert("mustache", "Mustache");
    m.insert("myghty", "Myghty");
    m.insert("nanorc", "INI");
    m.insert("nargo", "Noir");
    m.insert("nasal", "Nasal");
    m.insert("nasl", "NASL");
    m.insert("nasm", "Assembly");
    m.insert("ncl", "NCL");
    m.insert("ne-on", "NEON");
    m.insert("nearley", "Nearley");
    m.insert("nemerle", "Nemerle");
    m.insert("neon", "NEON");
    m.insert("neosnippet", "Vim Snippet");
    m.insert("nesc", "nesC");
    m.insert("netlinx", "NetLinx");
    m.insert("netlinx+erb", "NetLinx+ERB");
    m.insert("netlogo", "NetLogo");
    m.insert("nette object notation", "NEON");
    m.insert("newlisp", "NewLisp");
    m.insert("nextflow", "Nextflow");
    m.insert("nginx", "Nginx");
    m.insert("nginx configuration file", "Nginx");
    m.insert("nickel", "Nickel");
    m.insert("nim", "Nim");
    m.insert("ninja", "Ninja");
    m.insert("nit", "Nit");
    m.insert("nix", "Nix");
    m.insert("nixos", "Nix");
    m.insert("njk", "Nunjucks");
    m.insert("nl", "NL");
    m.insert("nmodl", "NMODL");
    m.insert("node", "JavaScript");
    m.insert("noir", "Noir");
    m.insert("npm config", "INI");
    m.insert("npmrc", "INI");
    m.insert("nroff", "Roff");
    m.insert("nsis", "NSIS");
    m.insert("nu", "Nu");
    m.insert("nu-script", "Nushell");
    m.insert("numpy", "Python");
    m.insert("nunjucks", "Nunjucks");
    m.insert("nush", "Nu");
    m.insert("nushell", "Nushell");
    m.insert("nushell-script", "Nushell");
    m.insert("nvim", "Vim Script");
    m.insert("nwscript", "NWScript");
    m.insert("oasv2", "OpenAPI Specification v2");
    m.insert("oasv2-json", "OpenAPI Specification v2");
    m.insert("oasv2-yaml", "OpenAPI Specification v2");
    m.insert("oasv3", "OpenAPI Specification v3");
    m.insert("oasv3-json", "OpenAPI Specification v3");
    m.insert("oasv3-yaml", "OpenAPI Specification v3");
    m.insert("oberon", "Oberon");
    m.insert("obj-c", "Objective-C");
    m.insert("obj-c++", "Objective-C++");
    m.insert("obj-j", "Objective-J");
    m.insert("objc", "Objective-C");
    m.insert("objc++", "Objective-C++");
    m.insert("objdump", "ObjDump");
    m.insert("object data instance notation", "Object Data Instance Notation");
    m.insert("objective-c", "Objective-C");
    m.insert("objective-c++", "Objective-C++");
    m.insert("objective-j", "Objective-J");
    m.insert("objectivec", "Objective-C");
    m.insert("objectivec++", "Objective-C++");
    m.insert("objectivej", "Objective-J");
    m.insert("objectpascal", "Pascal");
    m.insert("objectscript", "ObjectScript");
    m.insert("objj", "Objective-J");
    m.insert("ocaml", "OCaml");
    m.insert("octave", "MATLAB");
    m.insert("odin", "Odin");
    m.insert("odin-lang", "Odin");
    m.insert("odinlang", "Odin");
    m.insert("omgrofl", "Omgrofl");
    m.insert("omnet++ msg", "OMNeT++ MSG");
    m.insert("omnet++ ned", "OMNeT++ NED");
    m.insert("omnetpp-msg", "OMNeT++ MSG");
    m.insert("omnetpp-ned", "OMNeT++ NED");
    m.insert("oncrpc", "RPC");
    m.insert("ooc", "ooc");
    m.insert("opa", "Opa");
    m.insert("opal", "Opal");
    m.insert("open policy agent", "Open Policy Agent");
    m.insert("openapi specification v2", "OpenAPI Specification v2");
    m.insert("openapi specification v3", "OpenAPI Specification v3");
    m.insert("opencl", "C");
    m.insert("openedge", "OpenEdge ABL");
    m.insert("openedge abl", "OpenEdge ABL");
    m.insert("openqasm", "OpenQASM");
    m.insert("openrc", "Shell");
    m.insert("openrc runscript", "Shell");
    m.insert("openscad", "OpenSCAD");
    m.insert("openstep property list", "OpenStep Property List");
    m.insert("opentofu", "HCL");
    m.insert("opentype feature file", "OpenType Feature File");
    m.insert("option list", "Option List");
    m.insert("opts", "Option List");
    m.insert("org", "Org");
    m.insert("osascript", "AppleScript");
    m.insert("overpassql", "OverpassQL");
    m.insert("ox", "Ox");
    m.insert("oxygene", "Oxygene");
    m.insert("oz", "Oz");
    m.insert("p4", "P4");
    m.insert("pact", "Pact");
    m.insert("pan", "Pan");
    m.insert("pandoc", "Markdown");
    m.insert("papyrus", "Papyrus");
    m.insert("parrot", "Parrot");
    m.insert("parrot assembly", "Parrot");
    m.insert("parrot internal representation", "Parrot");
    m.insert("pascal", "Pascal");
    m.insert("pasm", "Parrot");
    m.insert("pawn", "Pawn");
    m.insert("pcbnew", "KiCad Layout");
    m.insert("pddl", "PDDL");
    m.insert("peg.js", "PEG.js");
    m.insert("pep8", "Pep8");
    m.insert("perl", "Perl");
    m.insert("perl-6", "Raku");
    m.insert("perl6", "Raku");
    m.insert("php", "PHP");
    m.insert("pic", "Roff");
    m.insert("pickle", "Pickle");
    m.insert("picolisp", "PicoLisp");
    m.insert("piglatin", "PigLatin");
    m.insert("pikchr", "Roff");
    m.insert("pike", "Pike");
    m.insert("pip requirements", "Pip Requirements");
    m.insert("pir", "Parrot");
    m.insert("pkl", "Pkl");
    m.insert("plain text", "Text");
    m.insert("plantuml", "PlantUML");
    m.insert("plpgsql", "PLpgSQL");
    m.insert("plsql", "PLSQL");
    m.insert("pod", "Pod");
    m.insert("pod 6", "Pod 6");
    m.insert("pogoscript", "PogoScript");
    m.insert("polar", "Polar");
    m.insert("pony", "Pony");
    m.insert("portugol", "Portugol");
    m.insert("posh", "PowerShell");
    m.insert("postcss", "CSS");
    m.insert("postscr", "PostScript");
    m.insert("postscript", "PostScript");
    m.insert("pot", "Gettext Catalog");
    m.insert("pov-ray", "POV-Ray SDL");
    m.insert("pov-ray sdl", "POV-Ray SDL");
    m.insert("povray", "POV-Ray SDL");
    m.insert("powerbuilder", "PowerBuilder");
    m.insert("powershell", "PowerShell");
    m.insert("praat", "Praat");
    m.insert("prisma", "Prisma");
    m.insert("processing", "Processing");
    m.insert("procfile", "Procfile");
    m.insert("progress", "OpenEdge ABL");
    m.insert("proguard", "Proguard");
    m.insert("prolog", "Prolog");
    m.insert("promela", "Promela");
    m.insert("propeller spin", "Propeller Spin");
    m.insert("proto", "Protocol Buffer");
    m.insert("protobuf", "Protocol Buffer");
    m.insert("protobuf text format", "Protocol Buffer Text Format");
    m.insert("protocol buffer", "Protocol Buffer");
    m.insert("protocol buffer text format", "Protocol Buffer Text Format");
    m.insert("protocol buffers", "Protocol Buffer");
    m.insert("public key", "Public Key");
    m.insert("pug", "Pug");
    m.insert("puppet", "Puppet");
    m.insert("pure data", "Pure Data");
    m.insert("purebasic", "PureBasic");
    m.insert("purescript", "PureScript");
    m.insert("pwsh", "PowerShell");
    m.insert("pycon", "Python");
    m.insert("pyret", "Pyret");
    m.insert("pyrex", "Cython");
    m.insert("python", "Python");
    m.insert("python console", "Python");
    m.insert("python traceback", "Python");
    m.insert("python3", "Python");
    m.insert("q", "q");
    m.insert("q#", "Q#");
    m.insert("qb", "QuickBASIC");
    m.insert("qb64", "QuickBASIC");
    m.insert("qbasic", "QuickBASIC");
    m.insert("ql", "CodeQL");
    m.insert("qmake", "QMake");
    m.insert("qml", "QML");
    m.insert("qsharp", "Q#");
    m.insert("qt script", "Qt Script");
    m.insert("quake", "Quake");
    m.insert("quakec", "QuakeC");
    m.insert("quickbasic", "QuickBASIC");
    m.insert("r", "R");
    m.insert("racket", "Racket");
    m.insert("ragel", "Ragel");
    m.insert("ragel-rb", "Ragel");
    m.insert("ragel-ruby", "Ragel");
    m.insert("rake", "Ruby");
    m.insert("raku", "Raku");
    m.insert("raml", "RAML");
    m.insert("rascal", "Rascal");
    m.insert("rascript", "RAScript");
    m.insert("raw", "Raw token data");
    m.insert("raw token data", "Raw token data");
    m.insert("razor", "HTML");
    m.insert("rb", "Ruby");
    m.insert("rbs", "Ruby");
    m.insert("rbx", "Ruby");
    m.insert("rdoc", "RDoc");
    m.insert("readline", "INI");
    m.insert("readline config", "INI");
    m.insert("realbasic", "REALbasic");
    m.insert("reason", "Reason");
    m.insert("reasonligo", "LigoLANG");
    m.insert("rebol", "Rebol");
    m.insert("record jar", "Record Jar");
    m.insert("red", "Red");
    m.insert("red/system", "Red");
    m.insert("redcode", "Redcode");
    m.insert("redirect rules", "Redirect Rules");
    m.insert("redirects", "Redirect Rules");
    m.insert("regex", "Regular Expression");
    m.insert("regexp", "Regular Expression");
    m.insert("regular expression", "Regular Expression");
    m.insert("ren'py", "Ren'Py");
    m.insert("renderscript", "RenderScript");
    m.insert("renpy", "Ren'Py");
    m.insert("rescript", "ReScript");
    m.insert("restructuredtext", "reStructuredText");
    m.insert("rexx", "REXX");
    m.insert("rez", "Rez");
    m.insert("rhtml", "HTML");
    m.insert("rich text format", "Rich Text Format");
    m.insert("ring", "Ring");
    m.insert("riot", "Riot");
    m.insert("rmarkdown", "RMarkdown");
    m.insert("robotframework", "RobotFramework");
    m.insert("robots", "robots.txt");
    m.insert("robots txt", "robots.txt");
    m.insert("robots.txt", "robots.txt");
    m.insert("roc", "Roc");
    m.insert("rocq", "Rocq Prover");
    m.insert("rocq prover", "Rocq Prover");
    m.insert("roff", "Roff");
    m.insert("roff manpage", "Roff");
    m.insert("ron", "RON");
    m.insert("ros interface", "ROS Interface");
    m.insert("rosmsg", "ROS Interface");
    m.insert("rouge", "Rouge");
    m.insert("routeros script", "RouterOS Script");
    m.insert("rpc", "RPC");
    m.insert("rpcgen", "RPC");
    m.insert("rpgle", "RPGLE");
    m.insert("rpm spec", "RPM Spec");
    m.insert("rs", "Rust");
    m.insert("rs-274x", "Gerber Image");
    m.insert("rscript", "R");
    m.insert("rss", "XML");
    m.insert("rst", "reStructuredText");
    m.insert("ruby", "Ruby");
    m.insert("runoff", "RUNOFF");
    m.insert("rust", "Rust");
    m.insert("rusthon", "Python");
    m.insert("sage", "Sage");
    m.insert("sail", "Sail");
    m.insert("salt", "SaltStack");
    m.insert("saltstack", "SaltStack");
    m.insert("saltstate", "SaltStack");
    m.insert("sarif", "JSON");
    m.insert("sas", "SAS");
    m.insert("sass", "Sass");
    m.insert("scala", "Scala");
    m.insert("scaml", "Scaml");
    m.insert("scenic", "Scenic");
    m.insert("scheme", "Scheme");
    m.insert("scilab", "Scilab");
    m.insert("scss", "SCSS");
    m.insert("sdc", "Tcl");
    m.insert("sed", "sed");
    m.insert("self", "Self");
    m.insert("selinux kernel policy language", "SELinux Policy");
    m.insert("selinux policy", "SELinux Policy");
    m.insert("sepolicy", "SELinux Policy");
    m.insert("sfv", "Checksums");
    m.insert("sh", "Shell");
    m.insert("shaderlab", "ShaderLab");
    m.insert("shell", "Shell");
    m.insert("shell-script", "Shell");
    m.insert("shellcheck config", "ShellCheck Config");
    m.insert("shellcheckrc", "ShellCheck Config");
    m.insert("shellsession", "ShellSession");
    m.insert("shen", "Shen");
    m.insert("sieve", "Sieve");
    m.insert("simple file verification", "Checksums");
    m.insert("singularity", "Singularity");
    m.insert("slang", "Slang");
    m.insert("slash", "Slash");
    m.insert("slice", "Slice");
    m.insert("slim", "Slim");
    m.insert("slint", "Slint");
    m.insert("smali", "Smali");
    m.insert("smalltalk", "Smalltalk");
    m.insert("smarty", "Smarty");
    m.insert("smithy", "Smithy");
    m.insert("sml", "Standard ML");
    m.insert("smpl", "SmPL");
    m.insert("smt", "SMT");
    m.insert("snakefile", "Python");
    m.insert("snakemake", "Python");
    m.insert("snipmate", "Vim Snippet");
    m.insert("snippet", "YASnippet");
    m.insert("solidity", "Solidity");
    m.insert("soong", "Soong");
    m.insert("sourcemod", "SourcePawn");
    m.insert("sourcepawn", "SourcePawn");
    m.insert("soy", "Closure Templates");
    m.insert("sparql", "SPARQL");
    m.insert("specfile", "RPM Spec");
    m.insert("spline font database", "Spline Font Database");
    m.insert("splus", "R");
    m.insert("sqf", "SQF");
    m.insert("sql", "SQL");
    m.insert("sqlpl", "SQLPL");
    m.insert("sqlrpgle", "RPGLE");
    m.insert("squeak", "Smalltalk");
    m.insert("squirrel", "Squirrel");
    m.insert("srecode template", "SRecode Template");
    m.insert("ssh config", "INI");
    m.insert("ssh_config", "INI");
    m.insert("sshconfig", "INI");
    m.insert("sshd_config", "INI");
    m.insert("sshdconfig", "INI");
    m.insert("stan", "Stan");
    m.insert("standard ml", "Standard ML");
    m.insert("star", "STAR");
    m.insert("starlark", "Starlark");
    m.insert("stata", "Stata");
    m.insert("stl", "STL");
    m.insert("stla", "STL");
    m.insert("ston", "Smalltalk");
    m.insert("stringtemplate", "StringTemplate");
    m.insert("stylus", "Stylus");
    m.insert("subrip text", "SubRip Text");
    m.insert("sugarss", "SugarSS");
    m.insert("sum", "Checksums");
    m.insert("sums", "Checksums");
    m.insert("supercollider", "SuperCollider");
    m.insert("surql", "SurrealQL");
    m.insert("surrealql", "SurrealQL");
    m.insert("survex data", "Survex data");
    m.insert("svelte", "Svelte");
    m.insert("svg", "SVG");
    m.insert("sway", "Sway");
    m.insert("sweave", "Sweave");
    m.insert("swift", "Swift");
    m.insert("swig", "SWIG");
    m.insert("systemverilog", "SystemVerilog");
    m.insert("tab-seperated values", "TSV");
    m.insert("tabular model definition language", "TMDL");
    m.insert("tact", "Tact");
    m.insert("talon", "Talon");
    m.insert("tcl", "Tcl");
    m.insert("tcsh", "Shell");
    m.insert("tea", "Tea");
    m.insert("teal", "Teal");
    m.insert("templ", "templ");
    m.insert("terra", "Terra");
    m.insert("terraform", "HCL");
    m.insert("terraform template", "HCL");
    m.insert("tex", "TeX");
    m.insert("texinfo", "Texinfo");
    m.insert("text", "Text");
    m.insert("text proto", "Protocol Buffer Text Format");
    m.insert("textgrid", "TextGrid");
    m.insert("textile", "Textile");
    m.insert("textmate properties", "TextMate Properties");
    m.insert("thrift", "Thrift");
    m.insert("ti program", "TI Program");
    m.insert("tl", "Type Language");
    m.insert("tl-verilog", "TL-Verilog");
    m.insert("tla", "TLA");
    m.insert("tm-properties", "TextMate Properties");
    m.insert("tmdl", "TMDL");
    m.insert("toit", "Toit");
    m.insert("toml", "TOML");
    m.insert("topojson", "JSON");
    m.insert("tor config", "Tor Config");
    m.insert("torrc", "Tor Config");
    m.insert("traveling salesman problem", "TSPLIB data");
    m.insert("travelling salesman problem", "TSPLIB data");
    m.insert("tree-sitter query", "Tree-sitter Query");
    m.insert("troff", "Roff");
    m.insert("ts", "TypeScript");
    m.insert("tsp", "TypeSpec");
    m.insert("tsplib data", "TSPLIB data");
    m.insert("tsq", "Tree-sitter Query");
    m.insert("tsql", "TSQL");
    m.insert("tsv", "TSV");
    m.insert("tsx", "TypeScript");
    m.insert("turing", "Turing");
    m.insert("turtle", "Turtle");
    m.insert("twig", "Twig");
    m.insert("txl", "TXL");
    m.insert("typ", "Typst");
    m.insert("type language", "Type Language");
    m.insert("typescript", "TypeScript");
    m.insert("typespec", "TypeSpec");
    m.insert("typst", "Typst");
    m.insert("udiff", "Diff");
    m.insert("ultisnip", "Vim Snippet");
    m.insert("ultisnips", "Vim Snippet");
    m.insert("unified parallel c", "C");
    m.insert("unity3d asset", "Unity3D Asset");
    m.insert("unix asm", "Assembly");
    m.insert("unix assembly", "Assembly");
    m.insert("uno", "Uno");
    m.insert("unrealscript", "UnrealScript");
    m.insert("untyped plutus core", "Untyped Plutus Core");
    m.insert("ur", "UrWeb");
    m.insert("ur/web", "UrWeb");
    m.insert("urweb", "UrWeb");
    m.insert("v", "V");
    m.insert("vala", "Vala");
    m.insert("valve data format", "Valve Data Format");
    m.insert("vb .net", "Visual Basic .NET");
    m.insert("vb 6", "Visual Basic 6.0");
    m.insert("vb.net", "Visual Basic .NET");
    m.insert("vb6", "Visual Basic 6.0");
    m.insert("vba", "VBA");
    m.insert("vbnet", "Visual Basic .NET");
    m.insert("vbscript", "VBScript");
    m.insert("vcard", "vCard");
    m.insert("vcl", "VCL");
    m.insert("vdf", "Valve Data Format");
    m.insert("velocity", "Velocity Template Language");
    m.insert("velocity template language", "Velocity Template Language");
    m.insert("vento", "Vento");
    m.insert("verilog", "Verilog");
    m.insert("vhdl", "VHDL");
    m.insert("vim", "Vim Script");
    m.insert("vim help file", "Vim Help File");
    m.insert("vim script", "Vim Script");
    m.insert("vim snippet", "Vim Snippet");
    m.insert("vimhelp", "Vim Help File");
    m.insert("viml", "Vim Script");
    m.insert("vimscript", "Vim Script");
    m.insert("virtual contact file", "vCard");
    m.insert("visual basic", "Visual Basic .NET");
    m.insert("visual basic .net", "Visual Basic .NET");
    m.insert("visual basic 6", "Visual Basic 6.0");
    m.insert("visual basic 6.0", "Visual Basic 6.0");
    m.insert("visual basic classic", "Visual Basic 6.0");
    m.insert("visual basic for applications", "VBA");
    m.insert("vlang", "V");
    m.insert("volt", "Volt");
    m.insert("vtl", "Velocity Template Language");
    m.insert("vtt", "WebVTT");
    m.insert("vue", "Vue");
    m.insert("vyper", "Vyper");
    m.insert("wasm", "WebAssembly");
    m.insert("wast", "WebAssembly");
    m.insert("wavefront material", "Wavefront Material");
    m.insert("wavefront object", "Wavefront Object");
    m.insert("wdl", "WDL");
    m.insert("web ontology language", "Web Ontology Language");
    m.insert("webassembly", "WebAssembly");
    m.insert("webassembly interface type", "WebAssembly Interface Type");
    m.insert("webidl", "WebIDL");
    m.insert("webvtt", "WebVTT");
    m.insert("wget config", "INI");
    m.insert("wgetrc", "INI");
    m.insert("wgsl", "WGSL");
    m.insert("whiley", "Whiley");
    m.insert("wiki", "Wikitext");
    m.insert("wikitext", "Wikitext");
    m.insert("win32 message file", "Win32 Message File");
    m.insert("winbatch", "Batchfile");
    m.insert("windows registry entries", "Windows Registry Entries");
    m.insert("wisp", "wisp");
    m.insert("wit", "WebAssembly Interface Type");
    m.insert("witcher script", "Witcher Script");
    m.insert("wl", "Wolfram Language");
    m.insert("wolfram", "Wolfram Language");
    m.insert("wolfram lang", "Wolfram Language");
    m.insert("wolfram language", "Wolfram Language");
    m.insert("wollok", "Wollok");
    m.insert("workflow description language", "WDL");
    m.insert("world of warcraft addon data", "World of Warcraft Addon Data");
    m.insert("wren", "Wren");
    m.insert("wrenlang", "Wren");
    m.insert("wsdl", "XML");
    m.insert("x bitmap", "C");
    m.insert("x font directory index", "X Font Directory Index");
    m.insert("x pixmap", "C");
    m.insert("x10", "X10");
    m.insert("xbase", "xBase");
    m.insert("xbm", "C");
    m.insert("xc", "XC");
    m.insert("xcompose", "XCompose");
    m.insert("xdc", "Tcl");
    m.insert("xdr", "RPC");
    m.insert("xhtml", "HTML");
    m.insert("xmake", "Xmake");
    m.insert("xml", "XML");
    m.insert("xml property list", "XML");
    m.insert("xml+genshi", "Genshi");
    m.insert("xml+kid", "Genshi");
    m.insert("xojo", "Xojo");
    m.insert("xonsh", "Xonsh");
    m.insert("xpages", "XPages");
    m.insert("xpm", "C");
    m.insert("xproc", "XProc");
    m.insert("xquery", "XQuery");
    m.insert("xs", "XS");
    m.insert("xsd", "XML");
    m.insert("xsl", "XSLT");
    m.insert("xslt", "XSLT");
    m.insert("xten", "X10");
    m.insert("xtend", "Xtend");
    m.insert("yacc", "Yacc");
    m.insert("yaml", "YAML");
    m.insert("yang", "YANG");
    m.insert("yara", "YARA");
    m.insert("yas", "YASnippet");
    m.insert("yasnippet", "YASnippet");
    m.insert("yml", "YAML");
    m.insert("yul", "Yul");
    m.insert("zap", "ZAP");
    m.insert("zeek", "Zeek");
    m.insert("zenscript", "ZenScript");
    m.insert("zephir", "Zephir");
    m.insert("zig", "Zig");
    m.insert("zil", "ZIL");
    m.insert("zimpl", "Zimpl");
    m.insert("zmodel", "Zmodel");
    m.insert("zsh", "Shell");
    m
});

/// Maps canonical language name -> all valid aliases (for validation)
pub static CANONICAL_TO_ALIASES: LazyLock<HashMap<&'static str, HashSet<&'static str>>> = LazyLock::new(|| {
    let mut m = HashMap::with_capacity(720);
    m.insert("1C Enterprise", HashSet::from(["1c enterprise"]));
    m.insert("2-Dimensional Array", HashSet::from(["2-dimensional array"]));
    m.insert("4D", HashSet::from(["4d"]));
    m.insert("ABAP", HashSet::from(["abap"]));
    m.insert("ABAP CDS", HashSet::from(["abap cds"]));
    m.insert("ABNF", HashSet::from(["abnf"]));
    m.insert("AGS Script", HashSet::from(["ags", "ags script"]));
    m.insert("AIDL", HashSet::from(["aidl"]));
    m.insert("AL", HashSet::from(["al"]));
    m.insert("ALGOL", HashSet::from(["algol"]));
    m.insert("AMPL", HashSet::from(["ampl"]));
    m.insert("ANTLR", HashSet::from(["antlr"]));
    m.insert("API Blueprint", HashSet::from(["api blueprint"]));
    m.insert("APL", HashSet::from(["apl"]));
    m.insert("ASL", HashSet::from(["asl"]));
    m.insert("ASN.1", HashSet::from(["asn.1"]));
    m.insert("ASP.NET", HashSet::from(["asp.net", "aspx", "aspx-vb"]));
    m.insert("ATS", HashSet::from(["ats", "ats2"]));
    m.insert(
        "ActionScript",
        HashSet::from(["actionscript", "actionscript 3", "actionscript3", "as3"]),
    );
    m.insert("Ada", HashSet::from(["ada", "ada2005", "ada95"]));
    m.insert(
        "Adblock Filter List",
        HashSet::from(["ad block", "ad block filters", "adb", "adblock", "adblock filter list"]),
    );
    m.insert(
        "Adobe Font Metrics",
        HashSet::from([
            "acfm",
            "adobe composite font metrics",
            "adobe font metrics",
            "adobe multiple font metrics",
            "amfm",
        ]),
    );
    m.insert("Agda", HashSet::from(["agda", "literate agda"]));
    m.insert("Aiken", HashSet::from(["aiken"]));
    m.insert("Alloy", HashSet::from(["alloy"]));
    m.insert("Altium Designer", HashSet::from(["altium", "altium designer"]));
    m.insert("AngelScript", HashSet::from(["angelscript"]));
    m.insert("Answer Set Programming", HashSet::from(["answer set programming"]));
    m.insert("Ant Build System", HashSet::from(["ant build system"]));
    m.insert("Antlers", HashSet::from(["antlers"]));
    m.insert("ApacheConf", HashSet::from(["aconf", "apache", "apacheconf"]));
    m.insert("Apex", HashSet::from(["apex"]));
    m.insert("AppleScript", HashSet::from(["applescript", "osascript"]));
    m.insert("Arc", HashSet::from(["arc"]));
    m.insert("AsciiDoc", HashSet::from(["asciidoc"]));
    m.insert("AspectJ", HashSet::from(["aspectj"]));
    m.insert(
        "Assembly",
        HashSet::from([
            "apollo guidance computer",
            "asm",
            "assembly",
            "gas",
            "gnu asm",
            "m68k",
            "motorola 68k assembly",
            "nasm",
            "unix asm",
            "unix assembly",
        ]),
    );
    m.insert("Astro", HashSet::from(["astro"]));
    m.insert("Asymptote", HashSet::from(["asymptote"]));
    m.insert("Augeas", HashSet::from(["augeas"]));
    m.insert("AutoHotkey", HashSet::from(["ahk", "autohotkey"]));
    m.insert("AutoIt", HashSet::from(["au3", "autoit", "autoit3", "autoitscript"]));
    m.insert("Avro IDL", HashSet::from(["avro idl"]));
    m.insert("Awk", HashSet::from(["awk"]));
    m.insert("B (Formal Method)", HashSet::from(["b (formal method)"]));
    m.insert("B4X", HashSet::from(["b4x", "basic for android"]));
    m.insert("BASIC", HashSet::from(["basic"]));
    m.insert("BQN", HashSet::from(["bqn"]));
    m.insert("Ballerina", HashSet::from(["ballerina"]));
    m.insert(
        "Batchfile",
        HashSet::from(["bat", "batch", "batchfile", "dosbatch", "winbatch"]),
    );
    m.insert("Beef", HashSet::from(["beef"]));
    m.insert("Befunge", HashSet::from(["befunge"]));
    m.insert("Berry", HashSet::from(["be", "berry"]));
    m.insert("BibTeX Style", HashSet::from(["bibtex style"]));
    m.insert("Bicep", HashSet::from(["bicep"]));
    m.insert("Bikeshed", HashSet::from(["bikeshed"]));
    m.insert("BitBake", HashSet::from(["bitbake"]));
    m.insert("Blade", HashSet::from(["blade"]));
    m.insert(
        "BlitzBasic",
        HashSet::from(["b3d", "blitz3d", "blitzbasic", "blitzplus", "bplus"]),
    );
    m.insert("BlitzMax", HashSet::from(["blitzmax", "bmax"]));
    m.insert(
        "Bluespec",
        HashSet::from([
            "bh",
            "bluespec",
            "bluespec bh",
            "bluespec bsv",
            "bluespec classic",
            "bsv",
        ]),
    );
    m.insert("Boo", HashSet::from(["boo"]));
    m.insert("Boogie", HashSet::from(["boogie"]));
    m.insert("Brainfuck", HashSet::from(["brainfuck"]));
    m.insert("BrighterScript", HashSet::from(["brighterscript"]));
    m.insert("Brightscript", HashSet::from(["brightscript"]));
    m.insert("Browserslist", HashSet::from(["browserslist"]));
    m.insert("Bru", HashSet::from(["bru"]));
    m.insert("BuildStream", HashSet::from(["buildstream"]));
    m.insert(
        "C",
        HashSet::from([
            "c",
            "opencl",
            "unified parallel c",
            "x bitmap",
            "x pixmap",
            "xbm",
            "xpm",
        ]),
    );
    m.insert("C#", HashSet::from(["c#", "cake", "cakescript", "csharp"]));
    m.insert("C++", HashSet::from(["c++", "cpp"]));
    m.insert("C-ObjDump", HashSet::from(["c-objdump"]));
    m.insert("C3", HashSet::from(["c3"]));
    m.insert("CAP CDS", HashSet::from(["cap cds", "cds"]));
    m.insert("CIL", HashSet::from(["cil"]));
    m.insert("CLIPS", HashSet::from(["clips"]));
    m.insert("CMake", HashSet::from(["cmake"]));
    m.insert("COBOL", HashSet::from(["cobol"]));
    m.insert("CODEOWNERS", HashSet::from(["codeowners"]));
    m.insert("COLLADA", HashSet::from(["collada"]));
    m.insert("CQL", HashSet::from(["cql"]));
    m.insert("CSON", HashSet::from(["cson"]));
    m.insert("CSS", HashSet::from(["css", "postcss"]));
    m.insert("CSV", HashSet::from(["csv"]));
    m.insert("CUE", HashSet::from(["cue"]));
    m.insert("CWeb", HashSet::from(["cweb"]));
    m.insert("Cabal Config", HashSet::from(["cabal", "cabal config"]));
    m.insert("Caddyfile", HashSet::from(["caddy", "caddyfile"]));
    m.insert("Cadence", HashSet::from(["cadence"]));
    m.insert("Cairo", HashSet::from(["cairo", "cairo zero"]));
    m.insert("Cangjie", HashSet::from(["cangjie"]));
    m.insert("Cap'n Proto", HashSet::from(["cap'n proto"]));
    m.insert("Carbon", HashSet::from(["carbon"]));
    m.insert("CartoCSS", HashSet::from(["carto", "cartocss"]));
    m.insert("Ceylon", HashSet::from(["ceylon"]));
    m.insert("Chapel", HashSet::from(["chapel", "chpl"]));
    m.insert("Charity", HashSet::from(["charity"]));
    m.insert(
        "Checksums",
        HashSet::from([
            "checksum",
            "checksums",
            "hash",
            "hashes",
            "sfv",
            "simple file verification",
            "sum",
            "sums",
        ]),
    );
    m.insert("ChucK", HashSet::from(["chuck"]));
    m.insert("Circom", HashSet::from(["circom"]));
    m.insert("Cirru", HashSet::from(["cirru"]));
    m.insert("Clarion", HashSet::from(["clarion"]));
    m.insert("Clarity", HashSet::from(["clarity"]));
    m.insert("Classic ASP", HashSet::from(["asp", "classic asp"]));
    m.insert("Clean", HashSet::from(["clean"]));
    m.insert("Click", HashSet::from(["click"]));
    m.insert("Clojure", HashSet::from(["clojure"]));
    m.insert("Closure Templates", HashSet::from(["closure templates", "soy"]));
    m.insert(
        "Cloud Firestore Security Rules",
        HashSet::from(["cloud firestore security rules"]),
    );
    m.insert("Clue", HashSet::from(["clue"]));
    m.insert("CoNLL-U", HashSet::from(["conll", "conll-u", "conll-x"]));
    m.insert("CodeQL", HashSet::from(["codeql", "ql"]));
    m.insert(
        "CoffeeScript",
        HashSet::from([
            "coffee",
            "coffee-script",
            "coffeescript",
            "litcoffee",
            "literate coffeescript",
        ]),
    );
    m.insert(
        "ColdFusion",
        HashSet::from(["cfc", "cfm", "cfml", "coldfusion", "coldfusion cfc", "coldfusion html"]),
    );
    m.insert("Common Lisp", HashSet::from(["common lisp", "lisp"]));
    m.insert(
        "Common Workflow Language",
        HashSet::from(["common workflow language", "cwl"]),
    );
    m.insert("Component Pascal", HashSet::from(["component pascal"]));
    m.insert("Cooklang", HashSet::from(["cooklang"]));
    m.insert("Cool", HashSet::from(["cool"]));
    m.insert("Cpp-ObjDump", HashSet::from(["c++-objdump", "cpp-objdump"]));
    m.insert("Creole", HashSet::from(["creole"]));
    m.insert("Crystal", HashSet::from(["crystal"]));
    m.insert("Csound", HashSet::from(["csound", "csound-orc"]));
    m.insert("Csound Document", HashSet::from(["csound document", "csound-csd"]));
    m.insert("Csound Score", HashSet::from(["csound score", "csound-sco"]));
    m.insert("Cuda", HashSet::from(["cuda"]));
    m.insert("Cue Sheet", HashSet::from(["cue sheet"]));
    m.insert("Curry", HashSet::from(["curry"]));
    m.insert("Cycript", HashSet::from(["cycript"]));
    m.insert("Cypher", HashSet::from(["cypher"]));
    m.insert("Cython", HashSet::from(["cython", "pyrex"]));
    m.insert("D", HashSet::from(["d", "dlang"]));
    m.insert("D-ObjDump", HashSet::from(["d-objdump"]));
    m.insert("D2", HashSet::from(["d2", "d2lang"]));
    m.insert(
        "DIGITAL Command Language",
        HashSet::from(["dcl", "digital command language"]),
    );
    m.insert("DM", HashSet::from(["byond", "dm"]));
    m.insert("DNS Zone", HashSet::from(["dns zone"]));
    m.insert("DTrace", HashSet::from(["dtrace", "dtrace-script"]));
    m.insert("Dafny", HashSet::from(["dafny"]));
    m.insert("Darcs Patch", HashSet::from(["darcs patch", "dpatch"]));
    m.insert("Dart", HashSet::from(["dart"]));
    m.insert("Daslang", HashSet::from(["daslang"]));
    m.insert("DataWeave", HashSet::from(["dataweave"]));
    m.insert(
        "Debian Package Control File",
        HashSet::from(["debian package control file"]),
    );
    m.insert("DenizenScript", HashSet::from(["denizenscript"]));
    m.insert("Dhall", HashSet::from(["dhall"]));
    m.insert("Diff", HashSet::from(["diff", "udiff"]));
    m.insert("DirectX 3D File", HashSet::from(["directx 3d file"]));
    m.insert("Dockerfile", HashSet::from(["containerfile", "dockerfile"]));
    m.insert("Dogescript", HashSet::from(["dogescript"]));
    m.insert("Dotenv", HashSet::from(["dotenv"]));
    m.insert("Dune", HashSet::from(["dune"]));
    m.insert("Dylan", HashSet::from(["dylan"]));
    m.insert("E", HashSet::from(["e"]));
    m.insert("E-mail", HashSet::from(["e-mail", "email", "eml", "mail", "mbox"]));
    m.insert("EBNF", HashSet::from(["ebnf"]));
    m.insert("ECL", HashSet::from(["ecl"]));
    m.insert("EJS", HashSet::from(["ejs"]));
    m.insert("EQ", HashSet::from(["eq"]));
    m.insert("Eagle", HashSet::from(["eagle"]));
    m.insert("Earthly", HashSet::from(["earthfile", "earthly"]));
    m.insert("Edge", HashSet::from(["edge"]));
    m.insert("EdgeQL", HashSet::from(["edgeql", "esdl"]));
    m.insert("Edje Data Collection", HashSet::from(["edje data collection"]));
    m.insert("Eiffel", HashSet::from(["eiffel"]));
    m.insert("Elixir", HashSet::from(["elixir"]));
    m.insert("Elm", HashSet::from(["elm"]));
    m.insert("Elvish", HashSet::from(["elvish", "elvish transcript"]));
    m.insert("Emacs Lisp", HashSet::from(["elisp", "emacs", "emacs lisp"]));
    m.insert("EmberScript", HashSet::from(["emberscript"]));
    m.insert("Erlang", HashSet::from(["erlang"]));
    m.insert("Euphoria", HashSet::from(["euphoria"]));
    m.insert("F#", HashSet::from(["f#", "fsharp"]));
    m.insert("F*", HashSet::from(["f*", "fstar"]));
    m.insert("FIGlet Font", HashSet::from(["figfont", "figlet font"]));
    m.insert("FIRRTL", HashSet::from(["firrtl"]));
    m.insert("FLUX", HashSet::from(["flux"]));
    m.insert("Factor", HashSet::from(["factor"]));
    m.insert("Fancy", HashSet::from(["fancy"]));
    m.insert("Fantom", HashSet::from(["fantom"]));
    m.insert("Faust", HashSet::from(["faust"]));
    m.insert("Fennel", HashSet::from(["fennel"]));
    m.insert("Filebench WML", HashSet::from(["filebench wml"]));
    m.insert("Flix", HashSet::from(["flix"]));
    m.insert("Fluent", HashSet::from(["fluent"]));
    m.insert("Formatted", HashSet::from(["formatted"]));
    m.insert("Forth", HashSet::from(["forth", "muf"]));
    m.insert("Fortran", HashSet::from(["fortran", "fortran free form"]));
    m.insert("FreeBASIC", HashSet::from(["fb", "freebasic"]));
    m.insert("FreeMarker", HashSet::from(["freemarker", "ftl"]));
    m.insert("Frege", HashSet::from(["frege"]));
    m.insert("Futhark", HashSet::from(["futhark"]));
    m.insert("G-code", HashSet::from(["g-code"]));
    m.insert("GAML", HashSet::from(["gaml"]));
    m.insert("GAMS", HashSet::from(["gams"]));
    m.insert("GAP", HashSet::from(["gap"]));
    m.insert("GCC Machine Description", HashSet::from(["gcc machine description"]));
    m.insert("GDB", HashSet::from(["gdb"]));
    m.insert("GDScript", HashSet::from(["gdscript"]));
    m.insert("GDShader", HashSet::from(["gdshader"]));
    m.insert("GEDCOM", HashSet::from(["gedcom"]));
    m.insert("GLSL", HashSet::from(["glsl"]));
    m.insert("GN", HashSet::from(["gn"]));
    m.insert("GSC", HashSet::from(["gsc"]));
    m.insert("Game Maker Language", HashSet::from(["game maker language"]));
    m.insert("Gemfile.lock", HashSet::from(["gemfile.lock"]));
    m.insert("Gemini", HashSet::from(["gemini", "gemtext"]));
    m.insert("Genero 4gl", HashSet::from(["genero 4gl"]));
    m.insert("Genero per", HashSet::from(["genero per"]));
    m.insert("Genie", HashSet::from(["genie"]));
    m.insert("Genshi", HashSet::from(["genshi", "xml+genshi", "xml+kid"]));
    m.insert("Gerber Image", HashSet::from(["gerber image", "rs-274x"]));
    m.insert("Gettext Catalog", HashSet::from(["gettext catalog", "pot"]));
    m.insert("Gherkin", HashSet::from(["cucumber", "gherkin"]));
    m.insert("Git Attributes", HashSet::from(["git attributes", "gitattributes"]));
    m.insert("Git Commit", HashSet::from(["commit", "git commit"]));
    m.insert(
        "Git Revision List",
        HashSet::from(["git blame ignore revs", "git revision list"]),
    );
    m.insert("Gleam", HashSet::from(["gleam"]));
    m.insert("Glyph", HashSet::from(["glyph"]));
    m.insert(
        "Glyph Bitmap Distribution Format",
        HashSet::from(["glyph bitmap distribution format"]),
    );
    m.insert("Gnuplot", HashSet::from(["gnuplot"]));
    m.insert("Go", HashSet::from(["go", "golang"]));
    m.insert(
        "Go Checksums",
        HashSet::from(["go checksums", "go sum", "go work sum", "go.sum", "go.work.sum"]),
    );
    m.insert("Go Module", HashSet::from(["go mod", "go module", "go.mod"]));
    m.insert("Go Template", HashSet::from(["go template", "gotmpl"]));
    m.insert("Go Workspace", HashSet::from(["go work", "go workspace", "go.work"]));
    m.insert("Godot Resource", HashSet::from(["godot resource"]));
    m.insert("Golo", HashSet::from(["golo"]));
    m.insert("Gosu", HashSet::from(["gosu"]));
    m.insert("Grace", HashSet::from(["grace"]));
    m.insert("Gradle", HashSet::from(["gradle", "gradle kotlin dsl"]));
    m.insert("Grammatical Framework", HashSet::from(["gf", "grammatical framework"]));
    m.insert("Graph Modeling Language", HashSet::from(["graph modeling language"]));
    m.insert("GraphQL", HashSet::from(["graphql"]));
    m.insert("Graphviz (DOT)", HashSet::from(["graphviz (dot)"]));
    m.insert(
        "Groovy",
        HashSet::from(["groovy", "groovy server pages", "gsp", "java server page"]),
    );
    m.insert("HAProxy", HashSet::from(["haproxy"]));
    m.insert(
        "HCL",
        HashSet::from([
            "hashicorp configuration language",
            "hcl",
            "opentofu",
            "terraform",
            "terraform template",
        ]),
    );
    m.insert("HIP", HashSet::from(["hip"]));
    m.insert("HLSL", HashSet::from(["hlsl"]));
    m.insert("HOCON", HashSet::from(["hocon"]));
    m.insert(
        "HTML",
        HashSet::from([
            "ecmarkdown",
            "ecmarkup",
            "ecr",
            "eex",
            "erb",
            "heex",
            "html",
            "html+ecr",
            "html+eex",
            "html+erb",
            "html+php",
            "html+razor",
            "html+ruby",
            "leex",
            "razor",
            "rhtml",
            "xhtml",
        ]),
    );
    m.insert("HTTP", HashSet::from(["http"]));
    m.insert("HXML", HashSet::from(["hxml"]));
    m.insert("Hack", HashSet::from(["hack"]));
    m.insert("Haml", HashSet::from(["haml"]));
    m.insert("Handlebars", HashSet::from(["handlebars", "hbs", "htmlbars"]));
    m.insert("Harbour", HashSet::from(["harbour"]));
    m.insert("Hare", HashSet::from(["hare"]));
    m.insert(
        "Haskell",
        HashSet::from(["c2hs", "c2hs haskell", "haskell", "lhaskell", "lhs", "literate haskell"]),
    );
    m.insert("Haxe", HashSet::from(["haxe"]));
    m.insert("HiveQL", HashSet::from(["hiveql"]));
    m.insert("HolyC", HashSet::from(["holyc"]));
    m.insert("Hosts File", HashSet::from(["hosts", "hosts file"]));
    m.insert("Hurl", HashSet::from(["hurl"]));
    m.insert("Hy", HashSet::from(["hy", "hylang"]));
    m.insert("HyPhy", HashSet::from(["hyphy"]));
    m.insert("IDL", HashSet::from(["idl"]));
    m.insert("IGOR Pro", HashSet::from(["igor", "igor pro", "igorpro"]));
    m.insert(
        "INI",
        HashSet::from([
            "curl config",
            "curlrc",
            "cylc",
            "dosini",
            "editor-config",
            "editorconfig",
            "git config",
            "gitconfig",
            "gitmodules",
            "ini",
            "inputrc",
            "nanorc",
            "npm config",
            "npmrc",
            "readline",
            "readline config",
            "ssh config",
            "ssh_config",
            "sshconfig",
            "sshd_config",
            "sshdconfig",
            "wget config",
            "wgetrc",
        ]),
    );
    m.insert("IRC log", HashSet::from(["irc", "irc log", "irc logs"]));
    m.insert("ISPC", HashSet::from(["ispc"]));
    m.insert("Idris", HashSet::from(["idris"]));
    m.insert(
        "Ignore List",
        HashSet::from(["git-ignore", "gitignore", "ignore", "ignore list"]),
    );
    m.insert("ImageJ Macro", HashSet::from(["ijm", "imagej macro"]));
    m.insert("Imba", HashSet::from(["imba"]));
    m.insert("Inform 7", HashSet::from(["i7", "inform 7", "inform7"]));
    m.insert("Ink", HashSet::from(["ink"]));
    m.insert("Inno Setup", HashSet::from(["inno setup"]));
    m.insert("Io", HashSet::from(["io"]));
    m.insert("Ioke", HashSet::from(["ioke"]));
    m.insert("Isabelle", HashSet::from(["isabelle", "isabelle root"]));
    m.insert("J", HashSet::from(["j"]));
    m.insert("JAR Manifest", HashSet::from(["jar manifest"]));
    m.insert("JCL", HashSet::from(["jcl"]));
    m.insert(
        "JSON",
        HashSet::from([
            "geojson",
            "json",
            "json with comments",
            "jsonc",
            "jsonl",
            "sarif",
            "topojson",
        ]),
    );
    m.insert("JSON5", HashSet::from(["json5"]));
    m.insert("JSONLD", HashSet::from(["jsonld"]));
    m.insert("JSONiq", HashSet::from(["jsoniq"]));
    m.insert("Jac", HashSet::from(["jac"]));
    m.insert("Jai", HashSet::from(["jai"]));
    m.insert("Janet", HashSet::from(["janet"]));
    m.insert("Jasmin", HashSet::from(["jasmin"]));
    m.insert(
        "Java",
        HashSet::from(["java", "java server pages", "java template engine", "jsp", "jte"]),
    );
    m.insert("Java Properties", HashSet::from(["java properties"]));
    m.insert(
        "JavaScript",
        HashSet::from([
            "ecere projects",
            "gjs",
            "glimmer js",
            "javascript",
            "javascript+erb",
            "js",
            "node",
        ]),
    );
    m.insert("Jest Snapshot", HashSet::from(["jest snapshot"]));
    m.insert("JetBrains MPS", HashSet::from(["jetbrains mps", "mps"]));
    m.insert(
        "Jinja",
        HashSet::from(["django", "html+django", "html+jinja", "htmldjango", "jinja"]),
    );
    m.insert("Jolie", HashSet::from(["jolie"]));
    m.insert("Jsonnet", HashSet::from(["jsonnet"]));
    m.insert("Julia", HashSet::from(["julia", "julia repl"]));
    m.insert(
        "Jupyter Notebook",
        HashSet::from(["ipython notebook", "jupyter notebook"]),
    );
    m.insert("Just", HashSet::from(["just", "justfile"]));
    m.insert("KCL", HashSet::from(["kcl"]));
    m.insert("KDL", HashSet::from(["kdl"]));
    m.insert("KFramework", HashSet::from(["kframework"]));
    m.insert("KRL", HashSet::from(["krl"]));
    m.insert("Kaitai Struct", HashSet::from(["kaitai struct", "ksy"]));
    m.insert("KakouneScript", HashSet::from(["kak", "kakounescript", "kakscript"]));
    m.insert("KerboScript", HashSet::from(["kerboscript"]));
    m.insert("KiCad Layout", HashSet::from(["kicad layout", "pcbnew"]));
    m.insert("KiCad Legacy Layout", HashSet::from(["kicad legacy layout"]));
    m.insert(
        "KiCad Schematic",
        HashSet::from(["eeschema schematic", "kicad schematic"]),
    );
    m.insert("Kickstart", HashSet::from(["kickstart"]));
    m.insert("Kit", HashSet::from(["kit"]));
    m.insert("KoLmafia ASH", HashSet::from(["kolmafia ash"]));
    m.insert("Koka", HashSet::from(["koka"]));
    m.insert("Kotlin", HashSet::from(["kotlin"]));
    m.insert("Kusto", HashSet::from(["kusto"]));
    m.insert("LFE", HashSet::from(["lfe"]));
    m.insert("LLVM", HashSet::from(["llvm"]));
    m.insert("LOLCODE", HashSet::from(["lolcode"]));
    m.insert("LSL", HashSet::from(["lsl"]));
    m.insert("LTspice Symbol", HashSet::from(["ltspice symbol"]));
    m.insert("LabVIEW", HashSet::from(["labview"]));
    m.insert("Lambdapi", HashSet::from(["lambdapi"]));
    m.insert("Langium", HashSet::from(["langium"]));
    m.insert("Lark", HashSet::from(["lark"]));
    m.insert("Lasso", HashSet::from(["lasso", "lassoscript"]));
    m.insert("Latte", HashSet::from(["latte"]));
    m.insert("Lean", HashSet::from(["lean", "lean 4", "lean4"]));
    m.insert("Leo", HashSet::from(["leo"]));
    m.insert("Less", HashSet::from(["less", "less-css"]));
    m.insert("Lex", HashSet::from(["flex", "jflex", "jison lex", "lex"]));
    m.insert("LigoLANG", HashSet::from(["cameligo", "ligolang", "reasonligo"]));
    m.insert("LilyPond", HashSet::from(["lilypond"]));
    m.insert("Limbo", HashSet::from(["limbo"]));
    m.insert("Linear Programming", HashSet::from(["linear programming"]));
    m.insert("Linker Script", HashSet::from(["linker script"]));
    m.insert("Linux Kernel Module", HashSet::from(["linux kernel module"]));
    m.insert("Liquid", HashSet::from(["liquid"]));
    m.insert("LiveCode Script", HashSet::from(["livecode script"]));
    m.insert("LiveScript", HashSet::from(["live-script", "livescript", "ls"]));
    m.insert("Logos", HashSet::from(["logos"]));
    m.insert("Logtalk", HashSet::from(["logtalk"]));
    m.insert("LookML", HashSet::from(["lookml"]));
    m.insert("LoomScript", HashSet::from(["loomscript"]));
    m.insert("Lua", HashSet::from(["lua"]));
    m.insert("Luau", HashSet::from(["luau"]));
    m.insert("M", HashSet::from(["m", "mumps"]));
    m.insert("M3U", HashSet::from(["hls playlist", "m3u", "m3u playlist"]));
    m.insert("M4", HashSet::from(["autoconf", "m4", "m4sugar"]));
    m.insert("MATLAB", HashSet::from(["matlab", "octave"]));
    m.insert("MAXScript", HashSet::from(["maxscript"]));
    m.insert("MDX", HashSet::from(["mdx"]));
    m.insert("MLIR", HashSet::from(["mlir"]));
    m.insert("MQL4", HashSet::from(["mql4"]));
    m.insert("MQL5", HashSet::from(["mql5"]));
    m.insert("MTML", HashSet::from(["mtml"]));
    m.insert("Macaulay2", HashSet::from(["m2", "macaulay2"]));
    m.insert("Makefile", HashSet::from(["bsdmake", "make", "makefile", "mf"]));
    m.insert("Mako", HashSet::from(["mako"]));
    m.insert("Markdown", HashSet::from(["markdown", "md", "pandoc"]));
    m.insert("Marko", HashSet::from(["marko", "markojs"]));
    m.insert("Mask", HashSet::from(["mask"]));
    m.insert(
        "Mathematical Programming System",
        HashSet::from(["mathematical programming system"]),
    );
    m.insert("Max", HashSet::from(["max", "max/msp", "maxmsp"]));
    m.insert("Mercury", HashSet::from(["mercury"]));
    m.insert("Mermaid", HashSet::from(["mermaid", "mermaid example"]));
    m.insert("Meson", HashSet::from(["meson"]));
    m.insert("Metal", HashSet::from(["metal"]));
    m.insert(
        "Microsoft Developer Studio Project",
        HashSet::from(["microsoft developer studio project"]),
    );
    m.insert(
        "Microsoft Visual Studio Solution",
        HashSet::from(["microsoft visual studio solution"]),
    );
    m.insert("MiniD", HashSet::from(["minid"]));
    m.insert("MiniYAML", HashSet::from(["miniyaml"]));
    m.insert("MiniZinc", HashSet::from(["minizinc"]));
    m.insert("MiniZinc Data", HashSet::from(["minizinc data"]));
    m.insert("Mint", HashSet::from(["mint"]));
    m.insert("Mirah", HashSet::from(["mirah"]));
    m.insert("Modelica", HashSet::from(["modelica"]));
    m.insert("Modula-2", HashSet::from(["modula-2"]));
    m.insert("Modula-3", HashSet::from(["modula-3"]));
    m.insert("Module Management System", HashSet::from(["module management system"]));
    m.insert("Mojo", HashSet::from(["mojo"]));
    m.insert("Monkey", HashSet::from(["monkey"]));
    m.insert("Monkey C", HashSet::from(["monkey c"]));
    m.insert("Moocode", HashSet::from(["moocode"]));
    m.insert("MoonBit", HashSet::from(["moonbit"]));
    m.insert("MoonScript", HashSet::from(["moonscript"]));
    m.insert("Motoko", HashSet::from(["motoko"]));
    m.insert("Move", HashSet::from(["move"]));
    m.insert("Muse", HashSet::from(["amusewiki", "emacs muse", "muse"]));
    m.insert("Mustache", HashSet::from(["mustache"]));
    m.insert("Myghty", HashSet::from(["myghty"]));
    m.insert("NASL", HashSet::from(["nasl"]));
    m.insert("NCL", HashSet::from(["ncl"]));
    m.insert("NEON", HashSet::from(["ne-on", "neon", "nette object notation"]));
    m.insert("NL", HashSet::from(["nl"]));
    m.insert("NMODL", HashSet::from(["nmodl"]));
    m.insert("NSIS", HashSet::from(["nsis"]));
    m.insert("NWScript", HashSet::from(["nwscript"]));
    m.insert("Nasal", HashSet::from(["nasal"]));
    m.insert("Nearley", HashSet::from(["nearley"]));
    m.insert("Nemerle", HashSet::from(["nemerle"]));
    m.insert("NetLinx", HashSet::from(["netlinx"]));
    m.insert("NetLinx+ERB", HashSet::from(["netlinx+erb"]));
    m.insert("NetLogo", HashSet::from(["netlogo"]));
    m.insert("NewLisp", HashSet::from(["newlisp"]));
    m.insert("Nextflow", HashSet::from(["nextflow"]));
    m.insert("Nginx", HashSet::from(["nginx", "nginx configuration file"]));
    m.insert("Nickel", HashSet::from(["nickel"]));
    m.insert("Nim", HashSet::from(["nim"]));
    m.insert("Ninja", HashSet::from(["ninja"]));
    m.insert("Nit", HashSet::from(["nit"]));
    m.insert("Nix", HashSet::from(["nix", "nixos"]));
    m.insert("Noir", HashSet::from(["nargo", "noir"]));
    m.insert("Nu", HashSet::from(["nu", "nush"]));
    m.insert("Nunjucks", HashSet::from(["njk", "nunjucks"]));
    m.insert("Nushell", HashSet::from(["nu-script", "nushell", "nushell-script"]));
    m.insert("OCaml", HashSet::from(["ocaml"]));
    m.insert("OMNeT++ MSG", HashSet::from(["omnet++ msg", "omnetpp-msg"]));
    m.insert("OMNeT++ NED", HashSet::from(["omnet++ ned", "omnetpp-ned"]));
    m.insert("Oberon", HashSet::from(["oberon"]));
    m.insert("ObjDump", HashSet::from(["objdump"]));
    m.insert(
        "Object Data Instance Notation",
        HashSet::from(["object data instance notation"]),
    );
    m.insert("ObjectScript", HashSet::from(["objectscript"]));
    m.insert(
        "Objective-C",
        HashSet::from(["obj-c", "objc", "objective-c", "objectivec"]),
    );
    m.insert(
        "Objective-C++",
        HashSet::from(["obj-c++", "objc++", "objective-c++", "objectivec++"]),
    );
    m.insert(
        "Objective-J",
        HashSet::from(["obj-j", "objective-j", "objectivej", "objj"]),
    );
    m.insert("Odin", HashSet::from(["odin", "odin-lang", "odinlang"]));
    m.insert("Omgrofl", HashSet::from(["omgrofl"]));
    m.insert("Opa", HashSet::from(["opa"]));
    m.insert("Opal", HashSet::from(["opal"]));
    m.insert("Open Policy Agent", HashSet::from(["open policy agent"]));
    m.insert(
        "OpenAPI Specification v2",
        HashSet::from(["oasv2", "oasv2-json", "oasv2-yaml", "openapi specification v2"]),
    );
    m.insert(
        "OpenAPI Specification v3",
        HashSet::from(["oasv3", "oasv3-json", "oasv3-yaml", "openapi specification v3"]),
    );
    m.insert(
        "OpenEdge ABL",
        HashSet::from(["abl", "openedge", "openedge abl", "progress"]),
    );
    m.insert("OpenQASM", HashSet::from(["openqasm"]));
    m.insert("OpenSCAD", HashSet::from(["openscad"]));
    m.insert("OpenStep Property List", HashSet::from(["openstep property list"]));
    m.insert(
        "OpenType Feature File",
        HashSet::from(["afdko", "opentype feature file"]),
    );
    m.insert("Option List", HashSet::from(["ackrc", "option list", "opts"]));
    m.insert("Org", HashSet::from(["org"]));
    m.insert("OverpassQL", HashSet::from(["overpassql"]));
    m.insert("Ox", HashSet::from(["ox"]));
    m.insert("Oxygene", HashSet::from(["oxygene"]));
    m.insert("Oz", HashSet::from(["oz"]));
    m.insert("P4", HashSet::from(["p4"]));
    m.insert("PDDL", HashSet::from(["pddl"]));
    m.insert("PEG.js", HashSet::from(["peg.js"]));
    m.insert("PHP", HashSet::from(["inc", "php"]));
    m.insert("PLSQL", HashSet::from(["plsql"]));
    m.insert("PLpgSQL", HashSet::from(["plpgsql"]));
    m.insert("POV-Ray SDL", HashSet::from(["pov-ray", "pov-ray sdl", "povray"]));
    m.insert("Pact", HashSet::from(["pact"]));
    m.insert("Pan", HashSet::from(["pan"]));
    m.insert("Papyrus", HashSet::from(["papyrus"]));
    m.insert(
        "Parrot",
        HashSet::from([
            "parrot",
            "parrot assembly",
            "parrot internal representation",
            "pasm",
            "pir",
        ]),
    );
    m.insert("Pascal", HashSet::from(["delphi", "objectpascal", "pascal"]));
    m.insert("Pawn", HashSet::from(["pawn"]));
    m.insert("Pep8", HashSet::from(["pep8"]));
    m.insert("Perl", HashSet::from(["cperl", "perl"]));
    m.insert("Pickle", HashSet::from(["pickle"]));
    m.insert("PicoLisp", HashSet::from(["picolisp"]));
    m.insert("PigLatin", HashSet::from(["piglatin"]));
    m.insert("Pike", HashSet::from(["pike"]));
    m.insert("Pip Requirements", HashSet::from(["pip requirements"]));
    m.insert("Pkl", HashSet::from(["pkl"]));
    m.insert("PlantUML", HashSet::from(["plantuml"]));
    m.insert("Pod", HashSet::from(["pod"]));
    m.insert("Pod 6", HashSet::from(["pod 6"]));
    m.insert("PogoScript", HashSet::from(["pogoscript"]));
    m.insert("Polar", HashSet::from(["polar"]));
    m.insert("Pony", HashSet::from(["pony"]));
    m.insert("Portugol", HashSet::from(["portugol"]));
    m.insert("PostScript", HashSet::from(["postscr", "postscript"]));
    m.insert("PowerBuilder", HashSet::from(["powerbuilder"]));
    m.insert("PowerShell", HashSet::from(["posh", "powershell", "pwsh"]));
    m.insert("Praat", HashSet::from(["praat"]));
    m.insert("Prisma", HashSet::from(["prisma"]));
    m.insert("Processing", HashSet::from(["processing"]));
    m.insert("Procfile", HashSet::from(["procfile"]));
    m.insert("Proguard", HashSet::from(["proguard"]));
    m.insert("Prolog", HashSet::from(["eclipse", "prolog"]));
    m.insert("Promela", HashSet::from(["promela"]));
    m.insert("Propeller Spin", HashSet::from(["propeller spin"]));
    m.insert(
        "Protocol Buffer",
        HashSet::from(["proto", "protobuf", "protocol buffer", "protocol buffers"]),
    );
    m.insert(
        "Protocol Buffer Text Format",
        HashSet::from(["protobuf text format", "protocol buffer text format", "text proto"]),
    );
    m.insert("Public Key", HashSet::from(["public key"]));
    m.insert("Pug", HashSet::from(["pug"]));
    m.insert("Puppet", HashSet::from(["puppet"]));
    m.insert("Pure Data", HashSet::from(["pure data"]));
    m.insert("PureBasic", HashSet::from(["purebasic"]));
    m.insert("PureScript", HashSet::from(["purescript"]));
    m.insert("Pyret", HashSet::from(["pyret"]));
    m.insert(
        "Python",
        HashSet::from([
            "easybuild",
            "numpy",
            "pycon",
            "python",
            "python console",
            "python traceback",
            "python3",
            "rusthon",
            "snakefile",
            "snakemake",
        ]),
    );
    m.insert("Q#", HashSet::from(["q#", "qsharp"]));
    m.insert("QML", HashSet::from(["qml"]));
    m.insert("QMake", HashSet::from(["qmake"]));
    m.insert("Qt Script", HashSet::from(["qt script"]));
    m.insert("Quake", HashSet::from(["quake"]));
    m.insert("QuakeC", HashSet::from(["quakec"]));
    m.insert(
        "QuickBASIC",
        HashSet::from([
            "classic qbasic",
            "classic quickbasic",
            "qb",
            "qb64",
            "qbasic",
            "quickbasic",
        ]),
    );
    m.insert("R", HashSet::from(["r", "rscript", "splus"]));
    m.insert("RAML", HashSet::from(["raml"]));
    m.insert("RAScript", HashSet::from(["rascript"]));
    m.insert("RDoc", HashSet::from(["rdoc"]));
    m.insert("REALbasic", HashSet::from(["realbasic"]));
    m.insert("REXX", HashSet::from(["arexx", "rexx"]));
    m.insert("RMarkdown", HashSet::from(["rmarkdown"]));
    m.insert("RON", HashSet::from(["ron"]));
    m.insert("ROS Interface", HashSet::from(["ros interface", "rosmsg"]));
    m.insert("RPC", HashSet::from(["oncrpc", "rpc", "rpcgen", "xdr"]));
    m.insert("RPGLE", HashSet::from(["ile rpg", "rpgle", "sqlrpgle"]));
    m.insert("RPM Spec", HashSet::from(["rpm spec", "specfile"]));
    m.insert("RUNOFF", HashSet::from(["runoff"]));
    m.insert("Racket", HashSet::from(["racket"]));
    m.insert("Ragel", HashSet::from(["ragel", "ragel-rb", "ragel-ruby"]));
    m.insert("Raku", HashSet::from(["perl-6", "perl6", "raku"]));
    m.insert("Rascal", HashSet::from(["rascal"]));
    m.insert("Raw token data", HashSet::from(["raw", "raw token data"]));
    m.insert("ReScript", HashSet::from(["rescript"]));
    m.insert("Reason", HashSet::from(["reason"]));
    m.insert("Rebol", HashSet::from(["rebol"]));
    m.insert("Record Jar", HashSet::from(["record jar"]));
    m.insert("Red", HashSet::from(["red", "red/system"]));
    m.insert("Redcode", HashSet::from(["redcode"]));
    m.insert("Redirect Rules", HashSet::from(["redirect rules", "redirects"]));
    m.insert(
        "Regular Expression",
        HashSet::from(["regex", "regexp", "regular expression"]),
    );
    m.insert("Ren'Py", HashSet::from(["ren'py", "renpy"]));
    m.insert("RenderScript", HashSet::from(["filterscript", "renderscript"]));
    m.insert("Rez", HashSet::from(["rez"]));
    m.insert("Rich Text Format", HashSet::from(["rich text format"]));
    m.insert("Ring", HashSet::from(["ring"]));
    m.insert("Riot", HashSet::from(["riot"]));
    m.insert("RobotFramework", HashSet::from(["robotframework"]));
    m.insert("Roc", HashSet::from(["roc"]));
    m.insert("Rocq Prover", HashSet::from(["coq", "rocq", "rocq prover"]));
    m.insert(
        "Roff",
        HashSet::from([
            "groff",
            "man",
            "man page",
            "man-page",
            "manpage",
            "mdoc",
            "nroff",
            "pic",
            "pikchr",
            "roff",
            "roff manpage",
            "troff",
        ]),
    );
    m.insert("Rouge", HashSet::from(["rouge"]));
    m.insert("RouterOS Script", HashSet::from(["routeros script"]));
    m.insert(
        "Ruby",
        HashSet::from(["jruby", "macruby", "rake", "rb", "rbs", "rbx", "ruby"]),
    );
    m.insert("Rust", HashSet::from(["rs", "rust"]));
    m.insert("SAS", HashSet::from(["sas"]));
    m.insert("SCSS", HashSet::from(["scss"]));
    m.insert(
        "SELinux Policy",
        HashSet::from(["selinux kernel policy language", "selinux policy", "sepolicy"]),
    );
    m.insert("SMT", HashSet::from(["smt"]));
    m.insert("SPARQL", HashSet::from(["sparql"]));
    m.insert("SQF", HashSet::from(["sqf"]));
    m.insert("SQL", HashSet::from(["sql"]));
    m.insert("SQLPL", HashSet::from(["sqlpl"]));
    m.insert("SRecode Template", HashSet::from(["srecode template"]));
    m.insert("STAR", HashSet::from(["star"]));
    m.insert("STL", HashSet::from(["ascii stl", "stl", "stla"]));
    m.insert("SVG", HashSet::from(["svg"]));
    m.insert("SWIG", HashSet::from(["swig"]));
    m.insert("Sage", HashSet::from(["sage"]));
    m.insert("Sail", HashSet::from(["sail"]));
    m.insert("SaltStack", HashSet::from(["salt", "saltstack", "saltstate"]));
    m.insert("Sass", HashSet::from(["sass"]));
    m.insert("Scala", HashSet::from(["scala"]));
    m.insert("Scaml", HashSet::from(["scaml"]));
    m.insert("Scenic", HashSet::from(["scenic"]));
    m.insert("Scheme", HashSet::from(["scheme"]));
    m.insert("Scilab", HashSet::from(["scilab"]));
    m.insert("Self", HashSet::from(["self"]));
    m.insert("ShaderLab", HashSet::from(["shaderlab"]));
    m.insert(
        "Shell",
        HashSet::from([
            "abuild",
            "alpine abuild",
            "apkbuild",
            "bash",
            "envrc",
            "fish",
            "gentoo ebuild",
            "gentoo eclass",
            "openrc",
            "openrc runscript",
            "sh",
            "shell",
            "shell-script",
            "tcsh",
            "zsh",
        ]),
    );
    m.insert(
        "ShellCheck Config",
        HashSet::from(["shellcheck config", "shellcheckrc"]),
    );
    m.insert(
        "ShellSession",
        HashSet::from(["bash session", "console", "shellsession"]),
    );
    m.insert("Shen", HashSet::from(["shen"]));
    m.insert("Sieve", HashSet::from(["sieve"]));
    m.insert("Singularity", HashSet::from(["singularity"]));
    m.insert("Slang", HashSet::from(["slang"]));
    m.insert("Slash", HashSet::from(["slash"]));
    m.insert("Slice", HashSet::from(["slice"]));
    m.insert("Slim", HashSet::from(["slim"]));
    m.insert("Slint", HashSet::from(["slint"]));
    m.insert("SmPL", HashSet::from(["coccinelle", "smpl"]));
    m.insert("Smali", HashSet::from(["smali"]));
    m.insert("Smalltalk", HashSet::from(["smalltalk", "squeak", "ston"]));
    m.insert("Smarty", HashSet::from(["smarty"]));
    m.insert("Smithy", HashSet::from(["smithy"]));
    m.insert("Solidity", HashSet::from(["solidity"]));
    m.insert("Soong", HashSet::from(["soong"]));
    m.insert("SourcePawn", HashSet::from(["sourcemod", "sourcepawn"]));
    m.insert("Spline Font Database", HashSet::from(["spline font database"]));
    m.insert("Squirrel", HashSet::from(["squirrel"]));
    m.insert("Stan", HashSet::from(["stan"]));
    m.insert("Standard ML", HashSet::from(["sml", "standard ml"]));
    m.insert("Starlark", HashSet::from(["bazel", "bzl", "starlark"]));
    m.insert("Stata", HashSet::from(["stata"]));
    m.insert("StringTemplate", HashSet::from(["stringtemplate"]));
    m.insert("Stylus", HashSet::from(["stylus"]));
    m.insert("SubRip Text", HashSet::from(["subrip text"]));
    m.insert("SugarSS", HashSet::from(["sugarss"]));
    m.insert("SuperCollider", HashSet::from(["supercollider"]));
    m.insert("SurrealQL", HashSet::from(["surql", "surrealql"]));
    m.insert("Survex data", HashSet::from(["survex data"]));
    m.insert("Svelte", HashSet::from(["svelte"]));
    m.insert("Sway", HashSet::from(["sway"]));
    m.insert("Sweave", HashSet::from(["sweave"]));
    m.insert("Swift", HashSet::from(["swift"]));
    m.insert("SystemVerilog", HashSet::from(["systemverilog"]));
    m.insert("TI Program", HashSet::from(["ti program"]));
    m.insert("TL-Verilog", HashSet::from(["tl-verilog"]));
    m.insert("TLA", HashSet::from(["tla"]));
    m.insert("TMDL", HashSet::from(["tabular model definition language", "tmdl"]));
    m.insert("TOML", HashSet::from(["toml"]));
    m.insert(
        "TSPLIB data",
        HashSet::from([
            "traveling salesman problem",
            "travelling salesman problem",
            "tsplib data",
        ]),
    );
    m.insert("TSQL", HashSet::from(["tsql"]));
    m.insert("TSV", HashSet::from(["tab-seperated values", "tsv"]));
    m.insert("TXL", HashSet::from(["txl"]));
    m.insert("Tact", HashSet::from(["tact"]));
    m.insert("Talon", HashSet::from(["talon"]));
    m.insert("Tcl", HashSet::from(["sdc", "tcl", "xdc"]));
    m.insert("TeX", HashSet::from(["bibtex", "latex", "tex"]));
    m.insert("Tea", HashSet::from(["tea"]));
    m.insert("Teal", HashSet::from(["teal"]));
    m.insert("Terra", HashSet::from(["terra"]));
    m.insert("Texinfo", HashSet::from(["texinfo"]));
    m.insert("Text", HashSet::from(["fundamental", "plain text", "text"]));
    m.insert("TextGrid", HashSet::from(["textgrid"]));
    m.insert(
        "TextMate Properties",
        HashSet::from(["textmate properties", "tm-properties"]),
    );
    m.insert("Textile", HashSet::from(["textile"]));
    m.insert("Thrift", HashSet::from(["thrift"]));
    m.insert("Toit", HashSet::from(["toit"]));
    m.insert("Tor Config", HashSet::from(["tor config", "torrc"]));
    m.insert("Tree-sitter Query", HashSet::from(["tree-sitter query", "tsq"]));
    m.insert("Turing", HashSet::from(["turing"]));
    m.insert("Turtle", HashSet::from(["turtle"]));
    m.insert("Twig", HashSet::from(["twig"]));
    m.insert("Type Language", HashSet::from(["tl", "type language"]));
    m.insert(
        "TypeScript",
        HashSet::from(["glimmer ts", "gts", "ts", "tsx", "typescript"]),
    );
    m.insert("TypeSpec", HashSet::from(["tsp", "typespec"]));
    m.insert("Typst", HashSet::from(["typ", "typst"]));
    m.insert("Unity3D Asset", HashSet::from(["unity3d asset"]));
    m.insert("Uno", HashSet::from(["uno"]));
    m.insert("UnrealScript", HashSet::from(["unrealscript"]));
    m.insert("Untyped Plutus Core", HashSet::from(["untyped plutus core"]));
    m.insert("UrWeb", HashSet::from(["ur", "ur/web", "urweb"]));
    m.insert("V", HashSet::from(["v", "vlang"]));
    m.insert("VBA", HashSet::from(["vba", "visual basic for applications"]));
    m.insert("VBScript", HashSet::from(["vbscript"]));
    m.insert("VCL", HashSet::from(["vcl"]));
    m.insert("VHDL", HashSet::from(["vhdl"]));
    m.insert("Vala", HashSet::from(["vala"]));
    m.insert(
        "Valve Data Format",
        HashSet::from(["keyvalues", "valve data format", "vdf"]),
    );
    m.insert(
        "Velocity Template Language",
        HashSet::from(["velocity", "velocity template language", "vtl"]),
    );
    m.insert("Vento", HashSet::from(["vento"]));
    m.insert("Verilog", HashSet::from(["verilog"]));
    m.insert("Vim Help File", HashSet::from(["help", "vim help file", "vimhelp"]));
    m.insert(
        "Vim Script",
        HashSet::from(["nvim", "vim", "vim script", "viml", "vimscript"]),
    );
    m.insert(
        "Vim Snippet",
        HashSet::from(["neosnippet", "snipmate", "ultisnip", "ultisnips", "vim snippet"]),
    );
    m.insert(
        "Visual Basic .NET",
        HashSet::from(["vb .net", "vb.net", "vbnet", "visual basic", "visual basic .net"]),
    );
    m.insert(
        "Visual Basic 6.0",
        HashSet::from([
            "classic visual basic",
            "vb 6",
            "vb6",
            "visual basic 6",
            "visual basic 6.0",
            "visual basic classic",
        ]),
    );
    m.insert("Volt", HashSet::from(["volt"]));
    m.insert("Vue", HashSet::from(["vue"]));
    m.insert("Vyper", HashSet::from(["vyper"]));
    m.insert("WDL", HashSet::from(["wdl", "workflow description language"]));
    m.insert("WGSL", HashSet::from(["wgsl"]));
    m.insert("Wavefront Material", HashSet::from(["wavefront material"]));
    m.insert("Wavefront Object", HashSet::from(["wavefront object"]));
    m.insert("Web Ontology Language", HashSet::from(["web ontology language"]));
    m.insert("WebAssembly", HashSet::from(["wasm", "wast", "webassembly"]));
    m.insert(
        "WebAssembly Interface Type",
        HashSet::from(["webassembly interface type", "wit"]),
    );
    m.insert("WebIDL", HashSet::from(["webidl"]));
    m.insert("WebVTT", HashSet::from(["vtt", "webvtt"]));
    m.insert("Whiley", HashSet::from(["whiley"]));
    m.insert("Wikitext", HashSet::from(["mediawiki", "wiki", "wikitext"]));
    m.insert("Win32 Message File", HashSet::from(["win32 message file"]));
    m.insert("Windows Registry Entries", HashSet::from(["windows registry entries"]));
    m.insert("Witcher Script", HashSet::from(["witcher script"]));
    m.insert(
        "Wolfram Language",
        HashSet::from([
            "mathematica",
            "mma",
            "wl",
            "wolfram",
            "wolfram lang",
            "wolfram language",
        ]),
    );
    m.insert("Wollok", HashSet::from(["wollok"]));
    m.insert(
        "World of Warcraft Addon Data",
        HashSet::from(["world of warcraft addon data"]),
    );
    m.insert("Wren", HashSet::from(["wren", "wrenlang"]));
    m.insert("X Font Directory Index", HashSet::from(["x font directory index"]));
    m.insert("X10", HashSet::from(["x10", "xten"]));
    m.insert("XC", HashSet::from(["xc"]));
    m.insert("XCompose", HashSet::from(["xcompose"]));
    m.insert(
        "XML",
        HashSet::from(["maven pom", "rss", "wsdl", "xml", "xml property list", "xsd"]),
    );
    m.insert("XPages", HashSet::from(["xpages"]));
    m.insert("XProc", HashSet::from(["xproc"]));
    m.insert("XQuery", HashSet::from(["xquery"]));
    m.insert("XS", HashSet::from(["xs"]));
    m.insert("XSLT", HashSet::from(["xsl", "xslt"]));
    m.insert("Xmake", HashSet::from(["xmake"]));
    m.insert("Xojo", HashSet::from(["xojo"]));
    m.insert("Xonsh", HashSet::from(["xonsh"]));
    m.insert("Xtend", HashSet::from(["xtend"]));
    m.insert("YAML", HashSet::from(["yaml", "yml"]));
    m.insert("YANG", HashSet::from(["yang"]));
    m.insert("YARA", HashSet::from(["yara"]));
    m.insert("YASnippet", HashSet::from(["snippet", "yas", "yasnippet"]));
    m.insert("Yacc", HashSet::from(["bison", "jison", "yacc"]));
    m.insert("Yul", HashSet::from(["yul"]));
    m.insert("ZAP", HashSet::from(["zap"]));
    m.insert("ZIL", HashSet::from(["zil"]));
    m.insert("Zeek", HashSet::from(["bro", "zeek"]));
    m.insert("ZenScript", HashSet::from(["zenscript"]));
    m.insert("Zephir", HashSet::from(["zephir"]));
    m.insert("Zig", HashSet::from(["zig"]));
    m.insert("Zimpl", HashSet::from(["zimpl"]));
    m.insert("Zmodel", HashSet::from(["zmodel"]));
    m.insert("crontab", HashSet::from(["cron", "cron table", "crontab"]));
    m.insert("desktop", HashSet::from(["desktop"]));
    m.insert("dircolors", HashSet::from(["dircolors"]));
    m.insert("eC", HashSet::from(["ec"]));
    m.insert("edn", HashSet::from(["edn"]));
    m.insert("hoon", HashSet::from(["hoon"]));
    m.insert("iCalendar", HashSet::from(["ical", "icalendar"]));
    m.insert("jq", HashSet::from(["jq"]));
    m.insert("kvlang", HashSet::from(["kvlang"]));
    m.insert("mIRC Script", HashSet::from(["mirc script"]));
    m.insert("mcfunction", HashSet::from(["mcfunction"]));
    m.insert("mdsvex", HashSet::from(["mdsvex"]));
    m.insert("mupad", HashSet::from(["mupad"]));
    m.insert("nesC", HashSet::from(["nesc"]));
    m.insert("ooc", HashSet::from(["ooc"]));
    m.insert("q", HashSet::from(["q"]));
    m.insert("reStructuredText", HashSet::from(["restructuredtext", "rst"]));
    m.insert("robots.txt", HashSet::from(["robots", "robots txt", "robots.txt"]));
    m.insert("sed", HashSet::from(["sed"]));
    m.insert("templ", HashSet::from(["templ"]));
    m.insert(
        "vCard",
        HashSet::from(["electronic business card", "vcard", "virtual contact file"]),
    );
    m.insert("wisp", HashSet::from(["wisp"]));
    m.insert("xBase", HashSet::from(["advpl", "clipper", "foxpro", "xbase"]));
    m
});

/// Preferred default alias for common languages (curated for widespread usage)
pub static DEFAULT_ALIASES: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::with_capacity(39);
    m.insert("C", "c");
    m.insert("C#", "csharp");
    m.insert("C++", "cpp");
    m.insert("CSS", "css");
    m.insert("Clojure", "clojure");
    m.insert("Dart", "dart");
    m.insert("Dockerfile", "dockerfile");
    m.insert("Elixir", "elixir");
    m.insert("Erlang", "erlang");
    m.insert("Go", "go");
    m.insert("GraphQL", "graphql");
    m.insert("HTML", "html");
    m.insert("Haskell", "haskell");
    m.insert("JSON", "json");
    m.insert("Java", "java");
    m.insert("JavaScript", "js");
    m.insert("Julia", "julia");
    m.insert("Kotlin", "kotlin");
    m.insert("Less", "less");
    m.insert("Lua", "lua");
    m.insert("Makefile", "makefile");
    m.insert("Markdown", "markdown");
    m.insert("Objective-C", "objc");
    m.insert("PHP", "php");
    m.insert("Perl", "perl");
    m.insert("Python", "python");
    m.insert("R", "r");
    m.insert("Ruby", "ruby");
    m.insert("Rust", "rust");
    m.insert("SCSS", "scss");
    m.insert("SQL", "sql");
    m.insert("Sass", "sass");
    m.insert("Scala", "scala");
    m.insert("Shell", "bash");
    m.insert("Swift", "swift");
    m.insert("TOML", "toml");
    m.insert("TypeScript", "ts");
    m.insert("XML", "xml");
    m.insert("YAML", "yaml");
    m
});

/// Resolve an alias to its canonical language name
#[inline]
pub fn resolve_canonical(alias: &str) -> Option<&'static str> {
    ALIAS_TO_CANONICAL.get(alias.to_lowercase().as_str()).copied()
}

/// Check if an alias is valid for a canonical language
#[inline]
pub fn is_valid_alias(canonical: &str, alias: &str) -> bool {
    CANONICAL_TO_ALIASES
        .get(canonical)
        .map(|aliases| aliases.contains(alias.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Get the preferred default alias for a language
#[inline]
pub fn default_alias(canonical: &str) -> Option<&'static str> {
    DEFAULT_ALIASES.get(canonical).copied()
}

/// Get all valid aliases for a canonical language
#[inline]
pub fn get_aliases(canonical: &str) -> Option<&HashSet<&'static str>> {
    CANONICAL_TO_ALIASES.get(canonical)
}

// Statistics:
// - 1208 aliases
// - 720 canonical languages
// - 39 curated default aliases
