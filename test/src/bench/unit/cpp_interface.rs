use rustyhgm_bench::cpp_interface::{declarations, section};

#[test]
fn a_declared_function_is_kept_inside_its_namespace() {
    assert_eq!(
        declarations("namespace diamond {\nstd::vector<std::string> rows(char letter);\n}\n"),
        "namespace diamond {\n    std::vector<std::string> rows(char letter);\n}"
    );
}

#[test]
fn guards_and_includes_are_left_out() {
    assert_eq!(
        declarations("#pragma once\n  #include <string>\n#if !defined(A_H)\nint f();\n#endif\n"),
        "int f();"
    );
}

#[test]
fn comments_are_left_out() {
    assert_eq!(
        declarations("// one\nint f(); /* two\n three */ int g(int half = 4 / 2);\n/* open"),
        "int f();\nint g(int half = 4 / 2);"
    );
}

#[test]
fn text_inside_a_literal_is_not_read_as_code() {
    assert_eq!(
        declarations("void f(std::string s = \"a//b{;\\\"\", char c = '}');\nint g();"),
        "void f(std::string s = \"a//b{;\\\"\", char c = '}');\nint g();"
    );
}

#[test]
fn an_unfinished_literal_runs_to_the_end() {
    assert_eq!(declarations("int f();\nchar c = 'x"), "int f();");
}

#[test]
fn a_function_body_is_replaced_by_a_semicolon() {
    assert_eq!(
        declarations(
            "inline int twice(int n)\n{\n    if (n) { return 2 * n; }\n    return 0;\n}\n"
        ),
        "inline int twice(int n);"
    );
}

#[test]
fn a_declaration_spread_over_lines_is_joined() {
    assert_eq!(
        declarations("int add(\n    int a,\n    int b );\n"),
        "int add(int a, int b);"
    );
}

#[test]
fn a_class_shows_only_its_public_members() {
    assert_eq!(
        declarations(
            "class account {\n    int hidden();\npublic:\n    void open();\nprotected:\n    void guard();\nprivate:\n    int balance_{0};\n public : int balance() { return balance_; }\n};\n"
        ),
        "class account {\npublic:\n    void open();\n    int balance();\n};"
    );
}

#[test]
fn a_struct_shows_its_members_until_a_private_section() {
    assert_eq!(
        declarations(
            "struct item {\n    int weight;\n    int value = 1;\nprivate:\n    int cache;\n};\n"
        ),
        "struct item {\n    int weight;\n    int value = 1;\n};"
    );
}

#[test]
fn a_union_is_read_like_a_struct() {
    assert_eq!(
        declarations("union cell { int number; char letter; };"),
        "union cell {\n    int number;\n    char letter;\n};"
    );
}

#[test]
fn a_private_nested_type_is_left_out() {
    assert_eq!(
        declarations(
            "class tree {\n    struct node { int value; };\n    enum side { left, right };\npublic:\n    int size() const;\n};\n"
        ),
        "class tree {\npublic:\n    int size() const;\n};"
    );
}

#[test]
fn a_public_nested_class_is_kept() {
    assert_eq!(
        declarations(
            "struct outer {\n    class inner {\n    public:\n        int get();\n    };\n};\n"
        ),
        "struct outer {\n    class inner {\n    public:\n        int get();\n    };\n};"
    );
}

#[test]
fn an_enum_keeps_its_names() {
    assert_eq!(
        declarations(
            "namespace n {\nenum class plants : char {\n    grass = 'G',\n    clover = 'C'\n};\n}\n"
        ),
        "namespace n {\n    enum class plants : char { grass = 'G', clover = 'C' };\n}"
    );
}

#[test]
fn a_constructor_loses_its_initializer_list() {
    assert_eq!(
        declarations(
            "struct clock {\n    clock(int hour, int minute)\n        : hour_(hour), minute_{minute}, zone_{0} {}\n    clock() : clock(0, 0) {}\n    int hour() const;\n};\n"
        ),
        "struct clock {\n    clock(int hour, int minute);\n    clock();\n    int hour() const;\n};"
    );
}

#[test]
fn a_braced_value_keeps_only_its_name() {
    assert_eq!(
        declarations(
            "const std::map<std::string, int> ALLERGENS {\n    {\"eggs\", 1},\n};\nconst std::vector<int> primes = {2, 3};\n"
        ),
        "const std::map<std::string, int> ALLERGENS;\nconst std::vector<int> primes;"
    );
}

#[test]
fn several_braced_values_in_one_statement_keep_the_first_name() {
    assert_eq!(
        declarations("int a{1}, b{2}, c;\nint d();\n"),
        "int a;\nint d();"
    );
}

#[test]
fn a_member_defined_outside_its_class_is_left_out() {
    assert_eq!(
        declarations(
            "namespace n {\nint list::size() const { return 0; }\nlist::~list() {}\nbool list::operator==(const list& o) const { return true; }\ntemplate <typename T>\nT buffer<T>::read() { return T(); }\nint free_size() { return 0; }\n}\n"
        ),
        "namespace n {\n    int free_size();\n}"
    );
}

#[test]
fn an_operator_defined_at_namespace_level_is_kept() {
    assert_eq!(
        declarations(
            "inline bool operator!=(const clock& a, const clock& b)\n{\n    return !(a == b);\n}\n"
        ),
        "inline bool operator!=(const clock& a, const clock& b);"
    );
}

#[test]
fn a_function_template_is_not_taken_for_a_class() {
    assert_eq!(
        declarations(
            "template <class T, class U = std::vector<T>>\nT first(const U& all) { return all[0]; }\n"
        ),
        "template <class T, class U = std::vector<T>> T first(const U& all);"
    );
}

#[test]
fn a_class_template_shows_its_public_members() {
    assert_eq!(
        declarations(
            "template <typename T>\nclass buffer {\npublic:\n    T read();\nprivate:\n    T item_;\n};\n"
        ),
        "template <typename T> class buffer {\npublic:\n    T read();\n};"
    );
}

#[test]
fn an_unfinished_template_list_is_read_as_a_value() {
    assert_eq!(
        declarations("template <typename T { int x; }"),
        "template <typename T;"
    );
}

#[test]
fn a_name_that_only_starts_like_a_keyword_is_not_one() {
    assert_eq!(
        declarations("templates t{1};\nprivateer p{2};\nstructure s{3};\n"),
        "templates t;\nprivateer p;\nstructure s;"
    );
}

#[test]
fn a_bare_block_leaves_nothing() {
    assert_eq!(declarations("{ int x; }\nint f();\n"), "int f();");
}

#[test]
fn an_unfinished_body_runs_to_the_end() {
    assert_eq!(declarations("int f() { return 1;\nint g();\n"), "int f();");
}

#[test]
fn a_header_without_declarations_adds_no_section() {
    assert_eq!(section(&["#pragma once\n// nothing\n", ""]), "");
}

#[test]
fn a_section_introduces_the_declarations_of_every_header() {
    assert_eq!(
        section(&["int f();\n", "", "int g();\n"]),
        "# Interface\n\nThe tests call the solution through the declarations below. Declare each one with exactly this name, these parameters and this return type. Everything else is yours to design.\n\n```cpp\nint f();\nint g();\n```\n\n"
    );
}
