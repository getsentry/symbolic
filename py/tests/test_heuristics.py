from symbolic import arch_get_ip_reg_name


def test_ip_reg():
    assert arch_get_ip_reg_name("armv7") == "pc"
    assert arch_get_ip_reg_name("x86") == "eip"
    assert arch_get_ip_reg_name("x86_64") == "rip"
