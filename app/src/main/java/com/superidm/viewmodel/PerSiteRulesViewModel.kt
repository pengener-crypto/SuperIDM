package com.superidm.viewmodel

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.superidm.data.db.PerSiteRuleDao
import com.superidm.data.db.PerSiteRuleEntity
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import javax.inject.Inject

@HiltViewModel
class PerSiteRulesViewModel @Inject constructor(
    private val perSiteRuleDao: PerSiteRuleDao
) : ViewModel() {
    
    val rules = perSiteRuleDao.getAll().stateIn(
        viewModelScope,
        SharingStarted.WhileSubscribed(5000L),
        emptyList()
    )
    
    fun addOrUpdateRule(entity: PerSiteRuleEntity) {
        viewModelScope.launch { perSiteRuleDao.insert(entity) }
    }
    
    fun deleteRule(domain: String) {
        viewModelScope.launch { perSiteRuleDao.deleteByDomain(domain) }
    }
}
